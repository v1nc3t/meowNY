package com.meowny.server.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.time.LocalDateTime;

import static org.assertj.core.api.Assertions.assertThat;

class CategoryGroupTest {

    @Test
    @DisplayName("Should verify all getters, setters, and created-at lifecycle")
    void testGettersSettersAndLifecycle() {
        CategoryGroup group = new CategoryGroup();
        User user = new User();

        group.setId(7L);
        group.setUser(user);
        group.setName("Living expenses");

        assertThat(group.getId()).isEqualTo(7L);
        assertThat(group.getUser()).isEqualTo(user);
        assertThat(group.getName()).isEqualTo("Living expenses");
        assertThat(group.getCreatedAt()).isNull();

        group.onCreate();
        assertThat(group.getCreatedAt()).isNotNull();
        assertThat(group.getCreatedAt()).isBeforeOrEqualTo(LocalDateTime.now());
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        CategoryGroup group = new CategoryGroup();
        assertThat(group.equals(group)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        CategoryGroup group = new CategoryGroup();
        assertThat(group.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        CategoryGroup group = new CategoryGroup();
        assertThat(group.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        CategoryGroup g1 = new CategoryGroup();
        CategoryGroup g2 = new CategoryGroup();
        assertThat(g1.equals(g2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        CategoryGroup g1 = new CategoryGroup();
        g1.setId(7L);
        CategoryGroup g2 = new CategoryGroup();

        assertThat(g1.equals(g2)).isFalse();
        assertThat(g2.equals(g1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        CategoryGroup g1 = new CategoryGroup();
        g1.setId(7L);
        CategoryGroup g2 = new CategoryGroup();
        g2.setId(7L);

        assertThat(g1.equals(g2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        CategoryGroup g1 = new CategoryGroup();
        g1.setId(7L);
        CategoryGroup g2 = new CategoryGroup();
        g2.setId(8L);

        assertThat(g1.equals(g2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        CategoryGroup g1 = new CategoryGroup();
        CategoryGroup g2 = new CategoryGroup();
        g2.setId(44L);

        assertThat(g1.hashCode()).isEqualTo(CategoryGroup.class.hashCode());
        assertThat(g2.hashCode()).isEqualTo(CategoryGroup.class.hashCode());
    }
}
