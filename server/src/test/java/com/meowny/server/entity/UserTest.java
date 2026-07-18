package com.meowny.server.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import java.util.ArrayList;
import java.util.List;
import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class UserTest {

    @Test
    @DisplayName("Should verify all getters, setters, and collection associations")
    void testGettersSettersAndCollectionMethods() {
        User user = new User();
        List<Transaction> transactions = new ArrayList<>();
        List<Budget> budgets = new ArrayList<>();

        user.setId(123L);
        user.setFirstName("Vincent");
        user.setLastName("Meowny");
        user.setEmail("vincent@meowny.com");
        user.setUsername("vincent_m");
        user.setPassword("hashed_pwd");
        user.setTransactions(transactions);
        user.setBudgets(budgets);

        assertThat(user.getId()).isEqualTo(123L);
        assertThat(user.getFirstName()).isEqualTo("Vincent");
        assertThat(user.getLastName()).isEqualTo("Meowny");
        assertThat(user.getFullName()).isEqualTo("Vincent Meowny");
        assertThat(user.getEmail()).isEqualTo("vincent@meowny.com");
        assertThat(user.getUsername()).isEqualTo("vincent_m");
        assertThat(user.getPassword()).isEqualTo("hashed_pwd");
        assertThat(user.getTransactions()).isEqualTo(transactions);
        assertThat(user.getBudgets()).isEqualTo(budgets);

        // Transaction collection helpers
        Transaction tx = new Transaction();
        user.addTransaction(tx);
        assertThat(user.getTransactions()).contains(tx);
        assertThat(tx.getUser()).isEqualTo(user);

        assertThatThrownBy(() -> user.addTransaction(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessage("Transaction cannot be null.");

        user.removeTransaction(tx);
        assertThat(user.getTransactions()).doesNotContain(tx);
        assertThat(tx.getUser()).isNull();

        user.removeTransaction(null); // No-op coverage

        // Budget collection helpers
        Budget budget = new Budget();
        user.addBudget(budget);
        assertThat(user.getBudgets()).contains(budget);
        assertThat(budget.getUser()).isEqualTo(user);

        assertThatThrownBy(() -> user.addBudget(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessage("Budget cannot be null.");

        user.removeBudget(budget);
        assertThat(user.getBudgets()).doesNotContain(budget);
        assertThat(budget.getUser()).isNull();

        user.removeBudget(null); // No-op coverage
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        User user = new User();
        assertThat(user.equals(user)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        User user = new User();
        assertThat(user.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        User user = new User();
        assertThat(user.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        User u1 = new User();
        User u2 = new User();
        assertThat(u1.equals(u2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        User u1 = new User();
        u1.setId(123L);
        User u2 = new User();

        assertThat(u1.equals(u2)).isFalse();
        assertThat(u2.equals(u1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        User u1 = new User();
        u1.setId(123L);
        User u2 = new User();
        u2.setId(123L);

        assertThat(u1.equals(u2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        User u1 = new User();
        u1.setId(123L);
        User u2 = new User();
        u2.setId(456L);

        assertThat(u1.equals(u2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        User u1 = new User();
        User u2 = new User();
        u2.setId(789L);

        assertThat(u1.hashCode()).isEqualTo(User.class.hashCode());
        assertThat(u2.hashCode()).isEqualTo(User.class.hashCode());
    }
}