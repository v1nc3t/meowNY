package com.meowny.commons.entity;

import jakarta.validation.ConstraintViolation;
import jakarta.validation.Validation;
import jakarta.validation.Validator;
import jakarta.validation.ValidatorFactory;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.Set;

import static org.assertj.core.api.Assertions.assertThat;

class TransactionTest {

    private Validator validator;
    private User user;
    private Category category;

    @BeforeEach
    void setUp() {
        try (ValidatorFactory factory = Validation.buildDefaultValidatorFactory()) {
            validator = factory.getValidator();
        }
        user = new User();
        user.setId(1L);

        category = new Category();
        category.setId(1L);
    }

    @Test
    void shouldPassValidationWithValidFields() {
        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setType(TransactionType.INCOME);
        tx.setName("Salary");
        tx.setAmount(new BigDecimal("5000.00"));
        tx.setPaymentDate(LocalDate.now());

        Set<ConstraintViolation<Transaction>> violations = validator.validate(tx);
        assertThat(violations).isEmpty();
    }

    @Test
    void shouldFailWhenAmountIsNegative() {
        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setType(TransactionType.EXPENSE);
        tx.setName("Coffee");
        tx.setAmount(new BigDecimal("-4.50"));
        tx.setPaymentDate(LocalDate.now());

        Set<ConstraintViolation<Transaction>> violations = validator.validate(tx);
        assertThat(violations).isNotEmpty();
    }

    @Test
    void shouldFailWhenPaymentDateIsInFuture() {
        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setType(TransactionType.EXPENSE);
        tx.setName("Future Rent");
        tx.setAmount(BigDecimal.TEN);
        tx.setPaymentDate(LocalDate.now().plusDays(1));

        Set<ConstraintViolation<Transaction>> violations = validator.validate(tx);
        assertThat(violations).isNotEmpty();
    }
}