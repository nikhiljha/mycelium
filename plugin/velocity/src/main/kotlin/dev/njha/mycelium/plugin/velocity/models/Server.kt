package dev.njha.mycelium.plugin.velocity.models

import kotlinx.serialization.*
import javax.annotation.Nullable

@Serializable
data class Server(val name: String, val address: String, @Nullable val host: String, @Nullable val priority: Int)
