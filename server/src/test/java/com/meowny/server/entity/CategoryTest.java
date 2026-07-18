package com.meowny.server.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import static org.assertj.core.api.Assertions.assertThat;

class CategoryTest {

    @Test
    @DisplayName("Should verify all getters and setters")
    void testGettersAndSetters() {
        Category category = new Category();
        User user = new User();

        category.setId(10L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Entertainment");

        assertThat(category.getId()).isEqualTo(10L);
        assertThat(category.getUser()).isEqualTo(user);
        assertThat(category.getType()).isEqualTo(TransactionType.EXPENSE);
        assertThat(category.getName()).isEqualTo("Entertainment");
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        Category category = new Category();
        assertThat(category.equals(category)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        Category category = new Category();
        assertThat(category.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        Category category = new Category();
        assertThat(category.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        Category c1 = new Category();
        Category c2 = new Category();
        assertThat(c1.equals(c2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        Category c1 = new Category();
        c1.setId(10L);
        Category c2 = new Category();

        assertThat(c1.equals(c2)).isFalse();
        assertThat(c2.equals(c1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        Category c1 = new Category();
        c1.setId(10L);
        Category c2 = new Category();
        c2.setId(10L);

        assertThat(c1.equals(c2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        Category c1 = new Category();
        c1.setId(10L);
        Category c2 = new Category();
        c2.setId(20L);

        assertThat(c1.equals(c2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        Category c1 = new Category();
        Category c2 = new Category();
        c2.setId(44L);

        assertThat(c1.hashCode()).isEqualTo(Category.class.hashCode());
        assertThat(c2.hashCode()).isEqualTo(Category.class.hashCode());
    }
}