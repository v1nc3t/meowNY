package client;

import client.config.Config;
import client.config.ConfigManager;
import client.scenes.MainCtrl;
import com.google.inject.Binder;
import com.google.inject.Module;
import com.google.inject.Scopes;

public class MyModule implements Module {

    private final String configPath;

    public MyModule(String configPath) {
        this.configPath = configPath;
    }

    @Override
    public void configure(Binder binder) {
        binder.bind(MainCtrl.class).in(Scopes.SINGLETON);
    }

    @Provides
    @Singleton
    public ConfigManager provideConfigManager() {
        ConfigManager configManager = new ConfigManager(configPath);
        configManager.loadConfig();
        return configManager;
    }

    @Provides
    public Config provideConfig(ConfigManager configManager) {
        return configManager.getConfig();
    }
}
