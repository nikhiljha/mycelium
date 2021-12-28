import dev.njha.mycelium.plugin.common.Monitoring
import org.bukkit.plugin.java.JavaPlugin

class Plugin : JavaPlugin() {
    override fun onEnable() {
        Monitoring().initMonitoring()
    }
}