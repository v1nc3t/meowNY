package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import java.math.BigDecimal;
import java.time.LocalDate;

import static org.assertj.core.api.Assertions.*;

public class ExpenseTest {

    private User user;
    private ExpenseCategory category;
    private RecurringExpense recurringExpense;
    private Expense expense;

    private LocalDate testDate = LocalDate.of(2026, 6, 27);

    @BeforeEach
    void setUp() {
        user = new User();
        user.setId(1L);

        category = new ExpenseCategory();
        category.setId(1L);
        category.setUser(user);

        recurringExpense = new RecurringExpense();
        recurringExpense.setId(1L);
        recurringExpense.setUser(user);

        expense = new Expense();
    }

    @Test
    void shouldCreateWithValidFields() {
        expense.setUser(user);
        expense.setName("Dinner");
        expense.setAmount(new BigDecimal("20.00"));
        expense.setPaymentDate(testDate);
        expense.setDescription("Hungry");
        expense.setSourceTemplate(recurringExpense);
        expense.setCategory(category);

        assertThat(expense.getName()).isEqualTo("Dinner");
        assertThat(expense.getAmount()).isEqualByComparingTo("20.00");
        assertThat(expense.getPaymentDate()).isEqualTo(testDate);
        assertThat(expense.getDescription()).isEqualTo("Hungry");

        assertThat(expense.getUser()).isEqualTo(user);
        assertThat(expense.getCategory()).isEqualTo(category);
        assertThat(expense.getSourceTemplate()).isEqualTo(recurringExpense);
    }

    @Test
    void shouldAllowZeroLimitAmount() {
        assertThatNoException().isThrownBy(() -> expense.setAmount(BigDecimal.ZERO));
    }

    // equals / hashCode

    @Test
    void shouldEqualSame() {
        Expense a = new Expense();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        Expense a = new Expense();
        a.setId(1L);
        Expense b = new Expense();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        Expense a = new Expense();
        a.setId(1L);
        Expense b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldBeEqualWithDifferentId() {
        Expense a = new Expense();
        a.setId(1L);
        Expense b = new Expense();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}
