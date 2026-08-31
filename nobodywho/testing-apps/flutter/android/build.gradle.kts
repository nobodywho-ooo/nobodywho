allprojects {
    repositories {
        System.getenv("NOBODYWHO_CANDIDATE_MAVEN_REPO")?.let { repositoryPath ->
            // CI's extracted release candidate resolves its native AAR here.
            // The directory exists only on the current Actions runner.
            maven { url = uri(repositoryPath) }
        }
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
subprojects {
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
