package com.meowny.commons.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import java.math.BigDecimal;
import static org.assertj.core.api.Assertions.assertThat;

class BudgetTest {

    @Test
    @DisplayName("Should verify all getters and setters")
    void testGettersAndSetters() {
        Budget budget = new Budget();
        User user = new User();
        Category category = new Category();
        BigDecimal limit = new BigDecimal("500.00");

        budget.setId(1L);
        budget.setUser(user);
        budget.setLimitAmount(limit);
        budget.setMonth(7);
        budget.setYear(2026);
        budget.setCategory(category);

        assertThat(budget.getId()).isEqualTo(1L);
        assertThat(budget.getUser()).isEqualTo(user);
        assertThat(budget.getLimitAmount()).isEqualTo(limit);
        assertThat(budget.getMonth()).isEqualTo(7);
        assertThat(budget.getYear()).isEqualTo(2026);
        assertThat(budget.getCategory()).isEqualTo(category);
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        Budget budget = new Budget();
        assertThat(budget.equals(budget)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        Budget budget = new Budget();
        assertThat(budget.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        Budget budget = new Budget();
        assertThat(budget.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        Budget b1 = new Budget();
        Budget b2 = new Budget();
        assertThat(b1.equals(b2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        Budget b1 = new Budget();
        b1.setId(1L);
        Budget b2 = new Budget();

        assertThat(b1.equals(b2)).isFalse();
        assertThat(b2.equals(b1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        Budget b1 = new Budget();
        b1.setId(1L);
        Budget b2 = new Budget();
        b2.setId(1L);

        assertThat(b1.equals(b2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        Budget b1 = new Budget();
        b1.setId(1L);
        Budget b2 = new Budget();
        b2.setId(2L);

        assertThat(b1.equals(b2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        Budget b1 = new Budget();
        Budget b2 = new Budget();
        b2.setId(99L);

        assertThat(b1.hashCode()).isEqualTo(Budget.class.hashCode());
        assertThat(b2.hashCode()).isEqualTo(Budget.class.hashCode());
    }
}