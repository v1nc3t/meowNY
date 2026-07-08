package com.meowny.commons.entity;

import jakarta.validation.ConstraintViolation;
import jakarta.validation.Validation;
import jakarta.validation.Validator;
import jakarta.validation.ValidatorFactory;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Set;

import static org.assertj.core.api.Assertions.assertThat;

class RecurringTransactionTest {

    private Validator validator;
    private User user;
    private Category category;

    @BeforeEach
    void setUp() {
        try (ValidatorFactory factory = Validation.buildDefaultValidatorFactory()) {
            validator = factory.getValidator();
        }
        user = new User();
        category = new Category();
    }

    @Test
    void shouldManageBidirectionalHistoryCorrectly() {
        RecurringTransaction template = new RecurringTransaction();
        Transaction tx = new Transaction();

        template.addTransaction(tx);

        assertThat(template.getHistory()).contains(tx);
        assertThat(tx.getSourceTemplate()).isEqualTo(template);
    }

    @Test
    void shouldFailValidationOnMissingRequiredFields() {
        RecurringTransaction template = new RecurringTransaction();

        Set<ConstraintViolation<RecurringTransaction>> violations = validator.validate(template);
        assertThat(violations).isNotEmpty();
    }
}