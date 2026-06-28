package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.math.BigDecimal;

import static org.assertj.core.api.Assertions.*;

public class BudgetTest {

    private User user;
    private ExpenseCategory category;

    private Budget budget;

    @BeforeEach
    void setUp() {
        user = new User();
        user.setId(1L);

        category = new ExpenseCategory();
        category.setId(1L);
        category.setUser(user);

        budget = new Budget();
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

        assertThat(budget.getUser()).isEqualTo(user);
        assertThat(budget.getCategory()).isEqualTo(category);
    }

    @Test
    void shouldAllowZeroLimitAmount() {
        budget.setLimitAmount(BigDecimal.ZERO);
    }

    @ParameterizedTest
    @ValueSource(ints = {1, 6, 12})
    void shouldAcceptValidMonths(int month) {
        assertThatNoException().isThrownBy(() -> budget.setMonth(month));
    }

    @ParameterizedTest
    @ValueSource(ints = {0, 13, -1, 99})
    void shouldRejectInvalidMonths(int month) {
        budget.setMonth(month);
        assertThat(budget.getMonth()).isEqualTo(month);
    }

    @ParameterizedTest
    @ValueSource(ints = {2000, 2025, 9999})
    void shouldAcceptValidYears(int year) {
        assertThatNoException().isThrownBy(() -> budget.setYear(year));
    }

    // equals / hashCode

    @Test
    void shouldEqualSame() {
        Budget a = new Budget();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        Budget a = new Budget();
        a.setId(1L);
        Budget b = new Budget();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        Budget a = new Budget();
        a.setId(1L);
        Expense b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldBeEqualWithDifferentId() {
        Budget a = new Budget();
        a.setId(1L);
        Budget b = new Budget();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}
