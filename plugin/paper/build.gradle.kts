plugins {
    kotlin("jvm") version "1.6.10"
    kotlin("kapt") version "1.6.10"
    id("com.github.johnrengelman.shadow")
}

group = "dev.njha.mycelium"
val myceliumVersion: String by rootProject.extra
version = myceliumVersion

repositories {
    mavenCentral()
    maven("https://papermc.io/repo/repository/maven-public/")
}

dependencies {
    implementation(kotlin("stdlib"))
    implementation(project(":common"))
    implementation("io.micrometer:micrometer-registry-prometheus:1.8.1")
    compileOnly("io.papermc.paper:paper-api:1.18.1-R0.1-SNAPSHOT")
    compileOnly("dev.cubxity.plugins:unifiedmetrics-api:0.3.4")
    kapt("io.papermc.paper:paper-api:1.18.1-R0.1-SNAPSHOT")
}

tasks {
    val shadowJar by getting(com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar::class) {
        archiveFileName.set("mycelium-${project.name}-${project.version}.jar")
    }
}
