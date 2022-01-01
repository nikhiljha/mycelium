import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    kotlin("jvm") version "1.6.10"
    kotlin("plugin.serialization") version "1.6.10"
    kotlin("kapt") version "1.6.10"
    id("com.github.johnrengelman.shadow")
}

group = "dev.njha.mycelium"
val myceliumVersion: String by rootProject.extra
version = myceliumVersion

repositories {
    mavenCentral()
    jcenter()
    maven(url= "https://nexus.velocitypowered.com/repository/maven-public/")
}

dependencies {
    testImplementation(kotlin("test-junit5"))
    testImplementation("org.junit.jupiter:junit-jupiter-api:5.8.2")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine:5.8.2")

    implementation("org.jetbrains.kotlin:kotlin-stdlib-jdk8")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.3.1")
    implementation("io.ktor:ktor-server-core:1.6.7")
    implementation("io.ktor:ktor-server-netty:1.6.7")
    implementation("io.ktor:ktor-client-core:1.6.7")
    implementation("io.ktor:ktor-client-java:1.6.7")
    implementation("io.ktor:ktor-gson:1.6.7")
    implementation("io.ktor:ktor-metrics-micrometer:1.6.7")
    implementation("io.micrometer:micrometer-registry-prometheus:1.8.1")

    compileOnly("com.velocitypowered:velocity-api:3.1.0")
    compileOnly("dev.cubxity.plugins:unifiedmetrics-api:0.3.4")
    kapt("com.velocitypowered:velocity-api:3.1.0")

    implementation(project(":common"))
}

tasks.test {
    useJUnitPlatform()
}

tasks.withType<KotlinCompile>() {
    kotlinOptions.jvmTarget = "16"
}

tasks.build {
    dependsOn("shadowJar")
}

tasks {
    val shadowJar by getting(com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar::class) {
        archiveFileName.set("mycelium-${project.name}-${project.version}.jar")
    }
}
