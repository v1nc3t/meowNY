package client.config;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import static com.fasterxml.jackson.databind.DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES;

public class ConfigManager {

    private static final String CONFIG_FILE = "config.json";

    private final ObjectMapper mapper;
    private final Path configPath;

    private Config config;

    public ConfigManager(String customConfigPath) {
        this.mapper = new ObjectMapper()
                .enable(SerializationFeature.INDENT_OUTPUT)
                .configure(FAIL_ON_UNKNOWN_PROPERTIES, false);

        if (customConfigPath != null) {
            this.configPath = Path.of(customConfigPath);
        } else {
            String userHome = System.getProperty("user.home");
            Path dirPath = Path.of(userHome, ".meowny");

            if(!Files.exists(dirPath)) {
                try {
                    Files.createDirectories(dirPath);
                } catch (IOException e) {
                    throw new RuntimeException("Failed to create config directory",e);
                }
            }
            this.configPath = dirPath.resolve(CONFIG_FILE);
        }
    }

    public Config getConfig() {
        return this.config;
    }

    public void setConfig(Config config) {
        this.config = config;
    }

    public void loadConfig() {
        try {
            if (!Files.exists(configPath)) {
                createDefaultConfig();
                return;
            }
            String json = Files.readString(configPath, StandardCharsets.UTF_8);
            this.config = mapper.readValue(json, Config.class);

            if (this.config == null) {
                createDefaultConfig();
            }
        } catch (IOException e) {
            createDefaultConfig();
        }
    }

    private void createDefaultConfig() {
        try {
            Files.createDirectories(configPath.getParent());
            this.config = createDefaultConfigObject();
            save();
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
    }

    public void save() {
        try {
            String json = mapper.writeValueAsString(this.config);
            Files.writeString(configPath, json, StandardCharsets.UTF_8);
        } catch (IOException e) {
            throw new RuntimeException("Failed to save config: " + e.getMessage(), e);
        }
    }

    private Config createDefaultConfigObject() {
        Config defaultConfig = new Config();
        defaultConfig.setServerAddress("http://localhost:8080/");
        defaultConfig.setMinRatio(0.40);
        defaultConfig.setDefaultRatio(0.60);
        return defaultConfig;
    }

}