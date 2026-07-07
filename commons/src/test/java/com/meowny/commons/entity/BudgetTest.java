package com.meowny.commons.entity;

import jakarta.validation.ConstraintViolation;
import jakarta.validation.Validation;
import jakarta.validation.Validator;
import jakarta.validation.ValidatorFactory;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.math.BigDecimal;
import java.util.Set;

import static org.assertj.core.api.Assertions.assertThat;

class BudgetTest {

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
    void shouldCreateBudgetWithValidFields() {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(new BigDecimal("100.00"));
        budget.setMonth(6);
        budget.setYear(2025);

        Set<ConstraintViolation<Budget>> violations = validator.validate(budget);
        assertThat(violations).isEmpty();
    }

    @ParameterizedTest
    @ValueSource(ints = {0, 13, -5})
    void shouldRejectInvalidMonthsViaValidator(int invalidMonth) {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(BigDecimal.ONE);
        budget.setYear(2026);
        budget.setMonth(invalidMonth);

        Set<ConstraintViolation<Budget>> violations = validator.validate(budget);
        assertThat(violations).isNotEmpty();
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        Budget a = new Budget();
        a.setId(1L);
        Budget b = new Budget();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}