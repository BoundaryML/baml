import com.intellij.openapi.components.Service
import com.intellij.openapi.project.Project
import com.intellij.util.messages.Topic


@Service(Service.Level.PROJECT)
class BamlGetPortService(private val project: Project) {

    companion object {
        val TOPIC = Topic.create(
            "BAML-port",
            Listener::class.java,
            Topic.BroadcastDirection.NONE
        )
    }

    @Volatile
    var port: Int? = null
        private set

    fun setPort(newPort: Int) {
        port = newPort
        project.messageBus
            .syncPublisher(TOPIC)
            .onPort(newPort)
    }

    fun interface Listener { fun onPort(port: Int) }
}

