package com.meowny.commons.entity;

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class UserTest {

    @Test
    void shouldHandleBidirectionalTransactionMapping() {
        User user = new User();
        Transaction tx = new Transaction();

        user.addTransaction(tx);

        assertThat(user.getTransactions()).contains(tx);
        assertThat(tx.getUser()).isEqualTo(user);
    }

    @Test
    void shouldThrowExceptionWhenAddingNullTransaction() {
        User user = new User();
        assertThatThrownBy(() -> user.addTransaction(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction cannot be null.");
    }

    @Test
    void shouldReturnCorrectFullName() {
        User user = new User();
        user.setFirstName("Vince");
        user.setLastName("McMeow");

        assertThat(user.getFullName()).isEqualTo("Vince McMeow");
    }
}