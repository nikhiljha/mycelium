package dev.njha.mycelium.plugin.common

import com.typesafe.config.ConfigFactory
import io.ktor.application.*
import io.ktor.config.*
import io.ktor.features.*
import io.ktor.gson.*
import io.ktor.http.*
import io.ktor.metrics.micrometer.*
import io.ktor.response.*
import io.ktor.routing.*
import io.ktor.server.engine.*
import io.ktor.server.netty.*
import io.micrometer.core.instrument.binder.jvm.JvmGcMetrics
import io.micrometer.core.instrument.binder.jvm.JvmMemoryMetrics
import io.micrometer.prometheus.PrometheusConfig
import io.micrometer.prometheus.PrometheusMeterRegistry
import org.slf4j.LoggerFactory

class Monitoring {
    fun initMonitoring(): PrometheusMeterRegistry {
        val appMicrometerRegistry = PrometheusMeterRegistry(PrometheusConfig.DEFAULT)

        val ews = embeddedServer(Netty, environment = applicationEngineEnvironment {
            log = LoggerFactory.getLogger("mycelium")
            config = HoconApplicationConfig(ConfigFactory.load())

            module {
                install(ContentNegotiation) {
                    gson()
                }
                install(MicrometerMetrics) {
                    meterBinders = listOf(
                        JvmMemoryMetrics(),
                        JvmGcMetrics()
                    )
                    registry = appMicrometerRegistry
                }
                routing {
                    get("/") {
                        call.respondText("ok", ContentType.Text.Plain)
                    }

                    get("/metrics") {
                        call.respond(appMicrometerRegistry.scrape())
                    }
                }
            }

            connector {
                port = 8080
                host = "0.0.0.0"
            }
        })
        ews.start(wait = false)
        return appMicrometerRegistry
    }
}