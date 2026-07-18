package com.meowny.server.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import java.math.BigDecimal;
import java.time.LocalDate;
import static org.assertj.core.api.Assertions.assertThat;

class RecurringTransactionTest {

    @Test
    @DisplayName("Should verify all getters, setters, and history collection mechanics")
    void testGettersSettersAndCollectionMethods() {
        RecurringTransaction rt = new RecurringTransaction();
        User user = new User();
        Category category = new Category();
        BigDecimal amount = new BigDecimal("29.99");
        LocalDate nextDue = LocalDate.now();

        rt.setId(5L);
        rt.setUser(user);
        rt.setType(TransactionType.EXPENSE);
        rt.setCategory(category);
        rt.setName("Gym Membership");
        rt.setAmount(amount);
        rt.setNextDueDate(nextDue);
        rt.setFrequency(Frequency.MONTHLY);
        rt.setActive(false);

        assertThat(rt.getId()).isEqualTo(5L);
        assertThat(rt.getUser()).isEqualTo(user);
        assertThat(rt.getType()).isEqualTo(TransactionType.EXPENSE);
        assertThat(rt.getCategory()).isEqualTo(category);
        assertThat(rt.getName()).isEqualTo("Gym Membership");
        assertThat(rt.getAmount()).isEqualTo(amount);
        assertThat(rt.getNextDueDate()).isEqualTo(nextDue);
        assertThat(rt.getFrequency()).isEqualTo(Frequency.MONTHLY);
        assertThat(rt.isActive()).isFalse();
        assertThat(rt.getHistory()).isNotNull().isEmpty();

        // Verify custom helper behavior
        Transaction tx = new Transaction();
        rt.addTransaction(tx);

        assertThat(rt.getHistory()).contains(tx);
        assertThat(tx.getSourceTemplate()).isEqualTo(rt);
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        RecurringTransaction rt = new RecurringTransaction();
        assertThat(rt.equals(rt)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        RecurringTransaction rt = new RecurringTransaction();
        assertThat(rt.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        RecurringTransaction rt = new RecurringTransaction();
        assertThat(rt.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        RecurringTransaction rt1 = new RecurringTransaction();
        RecurringTransaction rt2 = new RecurringTransaction();
        assertThat(rt1.equals(rt2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        RecurringTransaction rt1 = new RecurringTransaction();
        rt1.setId(5L);
        RecurringTransaction rt2 = new RecurringTransaction();

        assertThat(rt1.equals(rt2)).isFalse();
        assertThat(rt2.equals(rt1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        RecurringTransaction rt1 = new RecurringTransaction();
        rt1.setId(5L);
        RecurringTransaction rt2 = new RecurringTransaction();
        rt2.setId(5L);

        assertThat(rt1.equals(rt2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        RecurringTransaction rt1 = new RecurringTransaction();
        rt1.setId(5L);
        RecurringTransaction rt2 = new RecurringTransaction();
        rt2.setId(6L);

        assertThat(rt1.equals(rt2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        RecurringTransaction rt1 = new RecurringTransaction();
        RecurringTransaction rt2 = new RecurringTransaction();
        rt2.setId(99L);

        assertThat(rt1.hashCode()).isEqualTo(RecurringTransaction.class.hashCode());
        assertThat(rt2.hashCode()).isEqualTo(RecurringTransaction.class.hashCode());
    }
}