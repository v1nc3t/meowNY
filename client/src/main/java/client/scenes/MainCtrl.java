package client.scenes;

import javafx.scene.Parent;
import javafx.scene.Scene;
import javafx.stage.Screen;
import javafx.stage.Stage;
import javafx.util.Pair;

public class MainCtrl {

    private Stage primaryStage;

    public void initialize(Stage primaryStage) {
    private ApplicationCtrl applicationCtrl;
    private Scene applicationScene;


    public void initialize(Stage primaryStage, Pair<ApplicationCtrl, Parent> applicationOverview) {
        this.primaryStage = primaryStage;

        this.applicationCtrl = applicationOverview.getKey();
        this.applicationScene = new Scene(applicationOverview.getValue());

        

        showApplication();
        primaryStage.show();
    }
    public void showApplication() {
        primaryStage.setTitle("meowNY - Expense Tracker");
        primaryStage.setScene(applicationScene);
    }
}
