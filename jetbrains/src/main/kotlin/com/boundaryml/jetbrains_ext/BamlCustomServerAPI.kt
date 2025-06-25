import com.intellij.openapi.application.Application
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import org.eclipse.lsp4j.services.LanguageClient
import org.eclipse.lsp4j.services.LanguageServer
import java.util.concurrent.CompletableFuture;

data class PortParams(val port: Int)

interface BamlCustomServerAPI : LanguageClient {
    @JsonNotification("baml/port")
    fun onPort(params: PortParams)
}