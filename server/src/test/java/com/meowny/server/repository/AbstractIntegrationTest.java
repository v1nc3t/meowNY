package com.meowny.server.repository;

import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.testcontainers.containers.PostgreSQLContainer;

public abstract class AbstractIntegrationTest {

    @ServiceConnection
    protected static final PostgreSQLContainer<?> postgresContainer;

    static {
        postgresContainer = new PostgreSQLContainer<>("postgres:16-alpine")
                .withDatabaseName("meowny_test_db")
                .withUsername("meowny_user")
                .withPassword("meowny_pass");

        postgresContainer.start();
    }
}
