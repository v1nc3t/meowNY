package client.scenes;

import javafx.geometry.Rectangle2D;
import javafx.scene.Parent;
import javafx.scene.Scene;
import javafx.stage.Screen;
import javafx.stage.Stage;
import javafx.util.Pair;

public class MainCtrl {

    private Stage primaryStage;

    private ApplicationCtrl applicationCtrl;
    private Scene applicationScene;

    public static final double MIN_RATIO = 0.40;
    public static final double DEFAULT_RATIO = 0.60;

    public void initialize(Stage primaryStage, Pair<ApplicationCtrl, Parent> applicationOverview) {
        this.primaryStage = primaryStage;

        this.applicationCtrl = applicationOverview.getKey();
        this.applicationScene = new Scene(applicationOverview.getValue());

        setInitialSize();
        
        setupResizing();

        showApplication();
        primaryStage.show();
    }

    private void setInitialSize() {
        Rectangle2D screenBounds = Screen.getPrimary().getVisualBounds();
        double screenW = screenBounds.getWidth();
        double screenH = screenBounds.getHeight();
        primaryStage.setWidth(screenW * DEFAULT_RATIO);
        primaryStage.setHeight(screenH * DEFAULT_RATIO);
    }

    private void setupResizing() {
        primaryStage.xProperty().addListener(
                (observable, oldValue, newValue) -> updateMinConstraints()
        );
        primaryStage.yProperty().addListener(
                (observable, oldValue, newValue) -> updateMinConstraints()
        );

        updateMinConstraints();
    }

    private void updateMinConstraints() {
        var screens = Screen.getScreensForRectangle(
                primaryStage.getX(), primaryStage.getY(), primaryStage.getWidth(), primaryStage.getHeight()
        );

        if (!screens.isEmpty()) {
            Rectangle2D bounds = screens.getFirst().getVisualBounds();
            primaryStage.setMinWidth(bounds.getWidth() * MIN_RATIO);
            primaryStage.setMinHeight(bounds.getHeight() * MIN_RATIO);
        }
    }

    public void showApplication() {
        primaryStage.setTitle("meowNY - Expense Tracker");
        primaryStage.setScene(applicationScene);
    }
}
