package com.meowny.commons.entity;

import jakarta.validation.ConstraintViolation;
import jakarta.validation.Validation;
import jakarta.validation.Validator;
import jakarta.validation.ValidatorFactory;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.NullAndEmptySource;

import java.util.Set;

import static org.assertj.core.api.Assertions.assertThat;

class CategoryTest {

    private Validator validator;
    private User sampleUser;

    @BeforeEach
    void setUp() {
        try (ValidatorFactory factory = Validation.buildDefaultValidatorFactory()) {
            validator = factory.getValidator();
        }
        sampleUser = new User();
        sampleUser.setId(1L);
    }

    @Test
    void shouldPassValidationWithValidFields() {
        Category category = new Category();
        category.setUser(sampleUser);
        category.setType(TransactionType.EXPENSE);
        category.setName("Groceries");

        Set<ConstraintViolation<Category>> violations = validator.validate(category);
        assertThat(violations).isEmpty();
    }

    @Test
    void shouldFailWhenTypeIsNull() {
        Category category = new Category();
        category.setName("Utilities");
        category.setType(null);

        Set<ConstraintViolation<Category>> violations = validator.validate(category);
        assertThat(violations).hasSize(2);
    }

    @Test
    void shouldVerifyEqualsAndHashCodeContract() {
        Category cat1 = new Category();
        cat1.setId(100L);
        Category cat2 = new Category();
        cat2.setId(100L);
        Category cat3 = new Category();
        cat3.setId(200L);

        assertThat(cat1).isEqualTo(cat2);
        assertThat(cat1).isNotEqualTo(cat3);
        assertThat(cat1.hashCode()).isEqualTo(cat2.hashCode());
    }
}