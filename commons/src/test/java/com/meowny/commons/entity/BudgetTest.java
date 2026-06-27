package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.math.BigDecimal;

import static org.assertj.core.api.Assertions.*;

public class BudgetTest {

    private User user;
    private ExpenseCategory category;

    @BeforeEach
    void setUp() {
        user = new User();
        user.setId(1L);

        category = new ExpenseCategory();
        category.setId(1L);
        category.setUser(user);
    }

    @Test
    void shouldCreateBudgetWithValidFields() {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(new BigDecimal("100.00"));
        budget.setMonth(6);
        budget.setYear(2025);

        assertThat(budget.getLimitAmount()).isEqualByComparingTo("100.00");
        assertThat(budget.getMonth()).isEqualTo(6);
        assertThat(budget.getYear()).isEqualTo(2025);
    }

    @Test
    void shouldAllowZeroLimitAmount() {
        Budget budget = new Budget();
        budget.setLimitAmount(BigDecimal.ZERO);
    }

    @ParameterizedTest
    @ValueSource(ints = {1, 6, 12})
    void shouldAcceptValidMonths(int month) {
        Budget budget = new Budget();
        assertThatNoException().isThrownBy(() -> budget.setMonth(month));
    }

    @ParameterizedTest
    @ValueSource(ints = {0, 13, -1, 99})
    void shouldRejectInvalidMonths(int month) {
        Budget budget = new Budget();
        budget.setMonth(month);
        assertThat(budget.getMonth()).isEqualTo(month);
    }

    @ParameterizedTest
    @ValueSource(ints = {2000, 2025, 9999})
    void shouldAcceptValidYears(int year) {
        Budget budget = new Budget();
        assertThatNoException().isThrownBy(() -> budget.setYear(year));
    }

    @Test
    void validateOwnership_shouldPassWhenCategoryBelongsToSameUser() {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);

        assertThatNoException().isThrownBy(() -> invokeValidateOwnership(budget));
    }

    // Ownership validation (@PrePersist / @PreUpdate)

    @Test
    void validateOwnership_shouldThrowWhenCategoryBelongsToDifferentUser() {
        User otherUser = new User();
        otherUser.setId(2L);

        ExpenseCategory otherCategory = new ExpenseCategory();
        otherCategory.setId(2L);
        otherCategory.setUser(otherUser);

        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(otherCategory);

        assertThatThrownBy(() -> invokeValidateOwnership(budget))
                .isInstanceOf(IllegalStateException.class)
                .hasMessageContaining("does not belong to this user");
    }

    @Test
    void validateOwnership_shouldSkipWhenCategoryUserIsNull() {
        ExpenseCategory categoryNoUser = new ExpenseCategory();
        categoryNoUser.setId(3L);

        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(categoryNoUser);

        assertThatNoException().isThrownBy(() -> invokeValidateOwnership(budget));
    }

    // equals / hashCode

    @Test
    void equalsBudgets_shouldBeEqualWithSameNaturalKey() {
        Budget a = buildBudget(user, category, 6, 2025);
        Budget b = buildBudget(user, category, 6, 2025);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void equalsBudgets_shouldNotBeEqualWithDifferentMonth() {
        Budget a = buildBudget(user, category, 6, 2025);
        Budget b = buildBudget(user, category, 7, 2025);

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void equalsBudgets_shouldNotBeEqualWithDifferentYear() {
        Budget a = buildBudget(user, category, 6, 2025);
        Budget b = buildBudget(user, category, 6, 2026);

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void equalsBudgets_shouldNotBeEqualWithDifferentUser() {
        User otherUser = new User();
        otherUser.setId(99L);

        Budget a = buildBudget(user, category, 6, 2025);
        Budget b = buildBudget(otherUser, category, 6, 2025);

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void equalsBudgets_shouldBeReflexive() {
        Budget a = buildBudget(user, category, 6, 2025);
        assertThat(a).isEqualTo(a);
    }

    @Test
    void equalsBudgets_shouldNotBeEqualToNull() {
        Budget a = buildBudget(user, category, 6, 2025);
        assertThat(a).isNotEqualTo(null);
    }

    // Helpers

    private Budget buildBudget(User user, ExpenseCategory category, int month, int year) {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(new BigDecimal("200.00"));
        budget.setMonth(month);
        budget.setYear(year);
        return budget;
    }

    private void invokeValidateOwnership(Budget budget) throws Exception {
        var method = Budget.class.getDeclaredMethod("validateOwnership");
        method.setAccessible(true);
        try {
            method.invoke(budget);
        } catch (java.lang.reflect.InvocationTargetException e) {
            if (e.getCause() instanceof RuntimeException re) throw re;
            throw e;
        }
    }

}
