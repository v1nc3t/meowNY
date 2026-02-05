package client;

import java.io.IOException;
import java.net.URL;
import java.nio.file.Path;

import com.google.inject.Injector;

import javafx.fxml.FXMLLoader;
import javafx.scene.Parent;
import javafx.util.Builder;
import javafx.util.BuilderFactory;
import javafx.util.Callback;
import javafx.util.Pair;

public class MyFXML {

    private final Injector injector;

    public MyFXML(Injector injector) {
        this.injector = injector;
    }

    public <T> Pair<T, Parent> load(Class<T> c, String... parts) {
        String path = String.join("/", parts);
        URL location = MyFXML.class.getClassLoader().getResource(path);

        if (location == null) {
            throw new RuntimeException("FXML file not found at: " + path);
        }

        FXMLLoader loader = new FXMLLoader();
        loader.setLocation(location);
        loader.setControllerFactory(injector::getInstance);

        try {
            Parent parent = loader.load();
            T ctrl = loader.getController();
            return new Pair<>(ctrl, parent);
        } catch (IOException e) {
            throw new RuntimeException("Failed to load FXML: " + path, e);
        }
    }

    private URL getLocation(String... parts) {
        var path = Path.of("", parts).toString();
        return MyFXML.class.getClassLoader().getResource(path);
    }

    private class MyFactory implements BuilderFactory, Callback<Class<?>, Object> {

        @Override
        @SuppressWarnings("rawtypes")
        public Builder<?> getBuilder(Class<?> type) {
            return (Builder) () -> injector.getInstance(type);
        }

        @Override
        public Object call(Class<?> type) {
            return injector.getInstance(type);
        }
    }
}
