package dev.njha.mycelium.plugin.velocity.models

import kotlinx.serialization.*
import javax.annotation.Nullable

@Serializable
data class Server(val name: String, val address: String, @Nullable val host: String?, @Nullable val priority: Int?) : Comparable<Server> {
    override fun compareTo(other: Server): Int {
        if (priority == null && other.priority == null) {
            return 0
        } else if (priority == null) {
            return -1
        } else if (other.priority == null) {
            return 1
        } else if (priority < other.priority) {
            return -1
        } else if (other.priority > priority) {
            return 1
        }
        return 0
    }
}
