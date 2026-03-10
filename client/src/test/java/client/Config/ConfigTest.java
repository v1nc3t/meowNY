package client.Config;

import client.config.Config;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

public class ConfigTest {

    private Config config;

    double defaultRatio;

    @BeforeEach
    public void setUp() {
        config = new Config();
        defaultRatio = 0.6;
        config.setDefaultRatio(defaultRatio);
    }

    @Test
    public void testServerAddress() {
        String expectedServerAddress = "http://localhost:8080";
        config.setServerAddress(expectedServerAddress);
        assertEquals(expectedServerAddress, config.getServerAddress());
    }

    @Test
    public void testMinRatio() {
        double expectedMinRatio = 0.4;
        config.setMinRatio(expectedMinRatio);
        assertEquals(expectedMinRatio, config.getMinRatio());
    }

    @Test
    public void testDefaultRatio() {
        double expectedDefaultRatio = 0.6;
        config.setDefaultRatio(expectedDefaultRatio);
        assertEquals(expectedDefaultRatio, config.getDefaultRatio());
    }

}
