val myceliumVersion by extra { "0.5.0" }

plugins {
    id("com.github.johnrengelman.shadow") version "6.1.0" apply false
}

tasks.register("alljars") {
    dependsOn(":paper:shadowJar")
    dependsOn(":velocity:shadowJar")
}
