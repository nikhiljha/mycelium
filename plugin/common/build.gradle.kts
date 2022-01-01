plugins {
    kotlin("jvm") version "1.6.10"
}

group = "dev.njha.mycelium"
val myceliumVersion: String by rootProject.extra
version = myceliumVersion

repositories {
    mavenCentral()
}

dependencies {
    implementation(kotlin("stdlib"))
}