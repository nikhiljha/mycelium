package dev.njha.mycelium.plugin.velocity

import com.google.gson.Gson
import com.google.inject.Inject
import com.typesafe.config.ConfigFactory
import com.velocitypowered.api.event.Subscribe
import com.velocitypowered.api.event.proxy.ProxyInitializeEvent
import com.velocitypowered.api.event.proxy.ProxyShutdownEvent
import com.velocitypowered.api.plugin.Dependency
import com.velocitypowered.api.plugin.Plugin
import com.velocitypowered.api.plugin.annotation.DataDirectory
import com.velocitypowered.api.proxy.ProxyServer
import com.velocitypowered.api.proxy.server.ServerInfo
import dev.cubxity.plugins.metrics.api.UnifiedMetrics
import dev.cubxity.plugins.metrics.api.UnifiedMetricsProvider
import dev.njha.mycelium.plugin.velocity.metrics.MetricsCollection
import dev.njha.mycelium.plugin.velocity.metrics.MetricsCollector
import dev.njha.mycelium.plugin.velocity.models.Server
import io.ktor.application.*
import io.ktor.client.*
import io.ktor.client.engine.*
import io.ktor.client.engine.java.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import io.ktor.config.*
import io.ktor.features.*
import io.ktor.gson.*
import io.ktor.http.*
import io.ktor.request.*
import io.ktor.response.*
import io.ktor.routing.*
import io.ktor.server.engine.*
import io.ktor.server.netty.*
import kotlinx.coroutines.*
import org.slf4j.Logger
import org.slf4j.LoggerFactory
import java.net.ConnectException
import java.net.InetSocketAddress
import java.nio.file.Path
import java.util.*
import java.util.concurrent.TimeUnit
import kotlin.collections.set
import kotlin.reflect.full.declaredMemberFunctions
import kotlin.reflect.jvm.isAccessible


@Plugin(
    id = "mycelium",
    name = "Mycelium for Velocity",
    version = "0.4.0",
    dependencies = [Dependency(id = "unifiedmetrics", optional = false)],
    url = "https://nikhiljha.com/projects/mycelium",
    description = "syncs state with the Mycelium operator",
    authors = ["Nikhil Jha <source@nikhiljha.com>"]
)
class Plugin {
    @Inject
    lateinit var log: Logger

    @Inject
    lateinit var proxy: ProxyServer

    private lateinit var metrics: MetricsCollector

    @Inject
    @DataDirectory
    lateinit var dataFolderPath: Path

    private suspend fun sync() {
        // TODO: Generate a TLS cert for the API server
        HttpClient(Java).use { httpClient ->
            var churn = 0
            val endpoint = System.getenv("MYCELIUM_ENDPOINT") ?: "localhost:8181"
            val namespace = System.getenv("K8S_NAMESPACE") ?: "default"
            val name = System.getenv("K8S_NAME") ?: "proxy"
            val url = "http://$endpoint/servers/$namespace/$name"
            try {
                // if no env set, assume development and attempt to connect to localhost
                val response = httpClient.get<HttpResponse>(url) {
                    headers {
                        append("Accept", "application/json")
                    }
                }

                // parse servers
                val parsed = Gson().fromJson(response.readText(), Array<Server>::class.java)
                val newServers = HashMap<String, Server>()
                for (server in parsed) {
                    newServers[server.name] = server
                }

                // remove servers
                for (oldServer in proxy.allServers) {
                    if (!newServers.containsKey(oldServer.serverInfo.name)) {
                        proxy.configuration.attemptConnectionOrder.remove(oldServer.serverInfo.name);
                        proxy.unregisterServer(oldServer.serverInfo)
                        log.info("removed server ${oldServer.serverInfo.name}")
                        churn += 1
                    }
                }

                val forcedHosts = mutableMapOf<String, MutableList<String>>();
                val tryList: PriorityQueue<Server> = PriorityQueue<Server>();

                // add servers
                for (server in newServers.values) {
                    tryList.add(server)
                    val rs = proxy.getServer(server.name)
                    if (server.host != null) {
                        if (forcedHosts.containsKey(server.host)) {
                            forcedHosts[server.host]?.add(server.name)
                        } else {
                            forcedHosts[server.host] = mutableListOf(server.name)
                        }
                    }

                    if (rs.isEmpty) {
                        proxy.registerServer(
                            ServerInfo(
                                server.name,
                                InetSocketAddress(server.address, 25565)
                            )
                        )
                        log.info("added server ${server.name}")
                        churn += 1
                    }
                }

                // set default try list
                proxy.configuration.attemptConnectionOrder.clear()
                proxy.configuration.attemptConnectionOrder.addAll(tryList.map { server -> server.name })

                // set forced hosts
                val forcedHostsField = proxy.configuration::class.java.getDeclaredField("forcedHosts")
                forcedHostsField.isAccessible = true
                val fhClass = forcedHostsField.get(proxy.configuration)
                fhClass::class.declaredMemberFunctions.find { it.name == "setForcedHosts" }?.let {
                    it.isAccessible = true
                    it.call(fhClass, forcedHosts)
                }

                // record metrics
                metrics.churn = churn
            } catch (e: ConnectException) {
                log.error("failed to connect to operator - could not sync server list! (url = $url)")
            }
        }
    }

    @Subscribe
    fun onStart(event: ProxyInitializeEvent) {
        // hook into metrics ews
        metrics = MetricsCollector()
        if (proxy.pluginManager.isLoaded("unifiedmetrics")) {
            val api: UnifiedMetrics = UnifiedMetricsProvider.get()
            api.metricsManager.registerCollection(MetricsCollection(metrics))
        }

        // run mycelium api
        val ews = embeddedServer(Netty, environment = applicationEngineEnvironment {
            log = LoggerFactory.getLogger("mycelium")
            config = HoconApplicationConfig(ConfigFactory.load())

            module {
                install(ContentNegotiation) {
                    gson()
                }
                routing {
                    get("/") {
                        call.respondText("ok", ContentType.Text.Plain)
                    }

                    get("/debug/servers") {
                        call.respond(proxy.allServers)
                    }

                    get("/debug/config") {
                        call.respond(proxy.configuration)
                    }
                }
            }

            connector {
                port = 9273
                host = "0.0.0.0"
            }
        })
        ews.start(wait = false)

        // sync the servers from the operator now, and every 1 minute
        proxy.scheduler
            .buildTask(this) { runBlocking { launch { sync() } } }
            .repeat(1L, TimeUnit.MINUTES)
            .schedule()

        log.info("Hello, World.")
    }

    @Subscribe
    fun onStop(event: ProxyShutdownEvent) {
        log.info("Goodbye, World.")
    }
}