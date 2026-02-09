package client;

import client.scenes.ApplicationCtrl;
import client.scenes.MainCtrl;
import com.google.inject.Injector;
import javafx.application.Application;
import javafx.stage.Stage;

import java.io.IOException;
import java.net.URISyntaxException;

import static com.google.inject.Guice.createInjector;

public class Main extends Application {

    private Injector injector;
    private MyFXML fxml;

    public static void main(String[] args) throws URISyntaxException, IOException {
        launch(args);
    }

    @Override
    public void start(Stage primaryStage) throws Exception {
        String configPath = getParameters().getNamed().get("cfg");

        this.injector = createInjector(new MyModule(configPath));

        this.fxml = injector.getInstance(MyFXML.class);

        /*var serverUtils = INJECTOR.getInstance(ServerUtils.class);
        if (!serverUtils.isServerAvailable()) {
            var message = "Server needs to be started before the client, but it doesn't seem to be available. Shutting down.";
            System.err.println(message);
            return;
        }*/

        var applicationOverview = fxml.load(ApplicationCtrl.class, "client", "scenes", "Application.fxml");

        var mainCtrl = injector.getInstance(MainCtrl.class);
        mainCtrl.initialize(primaryStage, applicationOverview);
    }
}
