package client.config;

public class Config {

    private String serverAddress;
    private double minRatio;
    private double defaultRatio;

    public Config() {
    }

    public String getServerAddress() {
        return serverAddress;
    }

    public void setServerAddress(String serverAddress) {
        this.serverAddress = serverAddress;
    }

    public double getMinRatio() {
        return minRatio;
    }

    public void setMinRatio(double minRatio) {
        this.minRatio = minRatio;
    }

    public double getDefaultRatio() {
        return defaultRatio;
    }

    public void setDefaultRatio(double defaultRatio) {
        this.defaultRatio = defaultRatio;
    }
}
