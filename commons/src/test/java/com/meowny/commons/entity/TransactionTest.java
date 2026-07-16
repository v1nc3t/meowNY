package com.meowny.commons.entity;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import java.math.BigDecimal;
import java.time.LocalDate;
import static org.assertj.core.api.Assertions.assertThat;

class TransactionTest {

    @Test
    @DisplayName("Should verify all getters and setters")
    void testGettersAndSetters() {
        Transaction tx = new Transaction();
        User user = new User();
        Category category = new Category();
        RecurringTransaction template = new RecurringTransaction();
        BigDecimal amount = new BigDecimal("12.50");
        LocalDate paymentDate = LocalDate.now();

        tx.setId(100L);
        tx.setUser(user);
        tx.setType(TransactionType.EXPENSE);
        tx.setCategory(category);
        tx.setSourceTemplate(template);
        tx.setName("Lunch");
        tx.setAmount(amount);
        tx.setPaymentDate(paymentDate);
        tx.setDescription("Business meeting lunch");

        assertThat(tx.getId()).isEqualTo(100L);
        assertThat(tx.getUser()).isEqualTo(user);
        assertThat(tx.getType()).isEqualTo(TransactionType.EXPENSE);
        assertThat(tx.getCategory()).isEqualTo(category);
        assertThat(tx.getSourceTemplate()).isEqualTo(template);
        assertThat(tx.getName()).isEqualTo("Lunch");
        assertThat(tx.getAmount()).isEqualTo(amount);
        assertThat(tx.getPaymentDate()).isEqualTo(paymentDate);
        assertThat(tx.getDescription()).isEqualTo("Business meeting lunch");
    }

    @Test
    @DisplayName("Equals: Should return true when compared with itself")
    void testEqualsSelf() {
        Transaction tx = new Transaction();
        assertThat(tx.equals(tx)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with null")
    void testEqualsNull() {
        Transaction tx = new Transaction();
        assertThat(tx.equals(null)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when compared with a different class")
    void testEqualsDifferentClass() {
        Transaction tx = new Transaction();
        assertThat(tx.equals(new Object())).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when both entity IDs are null")
    void testEqualsBothIdsNull() {
        Transaction tx1 = new Transaction();
        Transaction tx2 = new Transaction();
        assertThat(tx1.equals(tx2)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return false when only one entity ID is null")
    void testEqualsOneIdNull() {
        Transaction tx1 = new Transaction();
        tx1.setId(100L);
        Transaction tx2 = new Transaction();

        assertThat(tx1.equals(tx2)).isFalse();
        assertThat(tx2.equals(tx1)).isFalse();
    }

    @Test
    @DisplayName("Equals: Should return true when both entity IDs are matching")
    void testEqualsMatchingIds() {
        Transaction tx1 = new Transaction();
        tx1.setId(100L);
        Transaction tx2 = new Transaction();
        tx2.setId(100L);

        assertThat(tx1.equals(tx2)).isTrue();
    }

    @Test
    @DisplayName("Equals: Should return false when entity IDs mismatch")
    void testEqualsDifferentIds() {
        Transaction tx1 = new Transaction();
        tx1.setId(100L);
        Transaction tx2 = new Transaction();
        tx2.setId(101L);

        assertThat(tx1.equals(tx2)).isFalse();
    }

    @Test
    @DisplayName("HashCode: Should return class name hash code consistently")
    void testHashCodeConsistency() {
        Transaction tx1 = new Transaction();
        Transaction tx2 = new Transaction();
        tx2.setId(1234L);

        assertThat(tx1.hashCode()).isEqualTo(Transaction.class.hashCode());
        assertThat(tx2.hashCode()).isEqualTo(Transaction.class.hashCode());
    }
}