package dev.njha.mycelium.velocity.models

import kotlinx.serialization.*
import javax.annotation.Nullable

@Serializable
data class Server(val name: String, val address: String, @Nullable val host: String)
