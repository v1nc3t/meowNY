package client.utils;

import jakarta.ws.rs.ProcessingException;
import jakarta.ws.rs.client.ClientBuilder;
import org.glassfish.jersey.client.ClientConfig;

import java.net.ConnectException;

import static jakarta.ws.rs.core.MediaType.APPLICATION_JSON;

public class ServerUtils {

    private static final String SERVER = "http://localhost:8080/";

    public boolean isServerAvailable() {
        try {
            ClientBuilder.newClient(new ClientConfig())
                    .target(SERVER)
                    .request(APPLICATION_JSON)
                    .get();
        } catch (ProcessingException e) {
            if (e.getCause() instanceof ConnectException) {
                return false;
            }
        }
        return true;
    }
}

