package com.meowny.server.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import java.time.LocalDateTime;
import static org.assertj.core.api.Assertions.assertThat;

class BaseAuditEntityTest {

    private static class TestAuditEntity extends BaseAuditEntity {}

    @Test
    @DisplayName("Should verify audit entity getters, pre-persist, and pre-update lifecycles")
    void testAuditLifecycleAndGetters() {
        TestAuditEntity entity = new TestAuditEntity();

        assertThat(entity.getCreatedAt()).isNull();
        assertThat(entity.getUpdatedAt()).isNull();

        entity.onCreate();
        LocalDateTime persistedCreated = entity.getCreatedAt();
        LocalDateTime persistedUpdated = entity.getUpdatedAt();

        assertThat(persistedCreated).isNotNull();
        assertThat(persistedUpdated).isNotNull();
        assertThat(persistedUpdated).isAfterOrEqualTo(persistedCreated);

        entity.onUpdate();
        assertThat(entity.getCreatedAt()).isEqualTo(persistedCreated);
        assertThat(entity.getUpdatedAt()).isAfterOrEqualTo(persistedUpdated);
    }
}