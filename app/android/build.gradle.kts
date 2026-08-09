allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}
// Every plugin compiles against the same JVM target as the app.
//
// Not tidiness. Gradle refuses to build a module whose Java and Kotlin tasks
// disagree, and `tflite_flutter` 0.12.1 declares Java 11 while the Kotlin
// plugin defaults to 21 — so adding it failed the whole Android build with
// "Inconsistent JVM-target compatibility", which names the module and not the
// cause. Pinning both compile tasks to the app's 17 fixes it for that plugin
// and for the next one that ships the same mismatch.
//
// Above `evaluationDependsOn(":app")` and without `afterEvaluate`, both
// deliberately: that call evaluates the subprojects as it goes, so anything
// registering an `afterEvaluate` below it arrives too late and Gradle says so.
// `configureEach` is lazy and applies to tasks that already exist.
subprojects {
    // Through the Android extension, not `tasks.withType<JavaCompile>`: the
    // Android plugin configures its own compile tasks from `compileOptions`
    // afterwards, so setting the tasks directly is silently overwritten and
    // leaves Java on 11 with Kotlin on 17 — the same error one notch along.
    // In `afterEvaluate`, because the plugin sets its own 11 in the body of
    // its build file: a `plugins.withId` callback fires when the plugin is
    // applied, which is *before* that, and is quietly overwritten.
    afterEvaluate {
        extensions.findByType(com.android.build.gradle.LibraryExtension::class.java)?.let {
            it.compileOptions.sourceCompatibility = JavaVersion.VERSION_17
            it.compileOptions.targetCompatibility = JavaVersion.VERSION_17
        }
    }
    tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
        compilerOptions {
            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        }
    }
}

subprojects {
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
