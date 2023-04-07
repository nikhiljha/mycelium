import com.typesafe.config.ConfigFactory
import io.ktor.server.application.*
import io.ktor.server.config.*
import io.ktor.serialization.gson.*
import io.ktor.http.*
import io.ktor.server.response.*
import io.ktor.server.routing.*
import io.ktor.server.engine.*
import io.ktor.server.netty.*
import io.ktor.server.plugins.contentnegotiation.*
import org.bukkit.plugin.java.JavaPlugin
import org.slf4j.LoggerFactory

class Plugin : JavaPlugin() {
    override fun onEnable() {
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
                }
            }

            connector {
                port = 9273
                host = "0.0.0.0"
            }
        })
        ews.start(wait = false)
    }
}