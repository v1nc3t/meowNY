package client.Config;

import client.config.Config;
import client.config.ConfigManager;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

public class ConfigManagerTest {

    @TempDir
    private Path tempDir;

    private Path customConfigPath;
    private ConfigManager configManager;

    @BeforeEach
    public void setUp() {
        customConfigPath = tempDir.resolve("customConfig.json");
        configManager = new ConfigManager(customConfigPath.toString());
    }

    @Test
    void testLoadCreateDefaultConfig() {
        configManager.loadConfig();

        assertNotNull(configManager.getConfig());
        assertTrue(Files.exists(customConfigPath), "Should create physical file for config");
    }

    @Test
    void testSaveLoadConsistency() {
        Config testConfig = new Config();
        testConfig.setServerAddress("https://example.com/");
        testConfig.setDefaultRatio(0.5);

        configManager.setConfig(testConfig);
        configManager.save();

        ConfigManager newConfigManager = new ConfigManager(customConfigPath.toString());
        newConfigManager.loadConfig();

        assertEquals("https://example.com/", newConfigManager.getConfig().getServerAddress());
        assertEquals(0.5, newConfigManager.getConfig().getDefaultRatio());
    }

    @Test
    void testBadJson() throws IOException {
        Files.writeString(customConfigPath, "{ \"invalid\": json }");

        configManager.loadConfig();

        assertNotNull(configManager.getConfig());
        assertEquals(0.60, configManager.getConfig().getDefaultRatio());
    }


}
