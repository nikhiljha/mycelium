import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    kotlin("jvm") version "1.5.20"
    kotlin("plugin.serialization") version "1.5.20"
    kotlin("kapt") version "1.5.20"
    id("com.github.johnrengelman.shadow") version "6.1.0"
}

group = "dev.njha.mycelium"
version = "0.1.0"

repositories {
    mavenCentral()
    jcenter()
    maven(url= "https://nexus.velocitypowered.com/repository/maven-public/")
}

dependencies {
    testImplementation(kotlin("test-junit5"))
    testImplementation("org.junit.jupiter:junit-jupiter-api:5.6.0")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine:5.6.0")

    implementation("org.jetbrains.kotlin:kotlin-stdlib-jdk8")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.2.2")
    implementation("io.ktor:ktor-server-core:1.6.1")
    implementation("io.ktor:ktor-server-netty:1.6.1")
    implementation("io.ktor:ktor-client-core:1.6.1")
    implementation("io.ktor:ktor-client-java:1.6.1")
    implementation("io.ktor:ktor-gson:1.6.1")
    compileOnly("com.velocitypowered:velocity-api:3.0.0")
    kapt("com.velocitypowered:velocity-api:3.0.0")
}

tasks.test {
    useJUnitPlatform()
}

tasks.withType<KotlinCompile>() {
    kotlinOptions.jvmTarget = "11"
}

tasks.build {
    dependsOn("shadowJar")
}
