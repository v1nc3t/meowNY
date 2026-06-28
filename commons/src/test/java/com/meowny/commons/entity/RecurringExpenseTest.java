package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;

import static org.assertj.core.api.Assertions.*;

public class RecurringExpenseTest {

    private User user;
    private ExpenseCategory expenseCategory;
    private LocalDate testDate = LocalDate.of(2020, 1, 1);

    private RecurringExpense recurringExpense;

    @BeforeEach
    void setup() {
        user = new User();
        user.setId(1L);

        expenseCategory = new ExpenseCategory();
        expenseCategory.setId(1L);
        expenseCategory.setUser(user);

        recurringExpense = new RecurringExpense();
    }

    @Test
    void shouldCreateWithValidFields() {
        recurringExpense.setUser(user);
        recurringExpense.setName("Test");
        recurringExpense.setAmount(new BigDecimal("1.00"));
        recurringExpense.setNextDueDate(testDate);
        recurringExpense.setFrequency(Frequency.DAILY);
        recurringExpense.setActive(true);
        recurringExpense.setCategory(expenseCategory);

        assertThat(recurringExpense.getUser()).isEqualTo(user);
        assertThat(recurringExpense.getName()).isEqualTo("Test");
        assertThat(recurringExpense.getAmount()).isEqualByComparingTo("1.00");
        assertThat(recurringExpense.getNextDueDate()).isEqualTo(testDate);
        assertThat(recurringExpense.getFrequency()).isEqualTo(Frequency.DAILY);
        assertThat(recurringExpense.isActive()).isTrue();
        assertThat(recurringExpense.getCategory()).isEqualTo(expenseCategory);
    }

    @Test
    void shouldAllowZeroLimitAmount() {
        assertThatNoException().isThrownBy(() -> recurringExpense.setAmount(BigDecimal.ZERO));
    }

    @Test
    void shouldAddRemovePaymentHistory() {
        Expense e10 = new Expense();
        e10.setId(10L);

        recurringExpense.setPaymentHistory(new ArrayList<>());
        recurringExpense.addPayment(e10);
        assertThat(recurringExpense.getPaymentHistory())
                .containsExactlyElementsOf(List.of(e10));
    }

    @Test
    void shouldNotAddNull() {
        assertThatThrownBy(() -> recurringExpense.addPayment(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("payment mustn't be null");
    }

    @Test
    void shouldRemovePaymentHistory() {
        Expense e11 = new Expense();
        e11.setId(11L);

        recurringExpense.addPayment(e11);
        recurringExpense.removePayment(e11);

        assertThat(recurringExpense.getPaymentHistory())
                .containsExactlyElementsOf(List.of());
    }

    @Test
    void shouldRemoveSourceTempalteExpense() {
        Expense e11 = new Expense();
        e11.setId(11L);

        e11.setSourceTemplate(recurringExpense);

        recurringExpense.addPayment(e11);
        recurringExpense.removePayment(e11);

        assertThat(recurringExpense.getPaymentHistory())
                .containsExactlyElementsOf(List.of());
        assertThat(e11.getSourceTemplate()).isNull();
    }

    @Test
    void shouldDoNothingRemoveNull() {
        Expense e11 = new Expense();
        e11.setId(11L);

        recurringExpense.addPayment(e11);
        recurringExpense.removePayment(null);

        assertThat(recurringExpense.getPaymentHistory())
                .containsExactlyElementsOf(List.of(e11));
        assertThat(e11.getSourceTemplate()).isNotNull();
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        RecurringExpense a = new RecurringExpense();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        RecurringExpense a = new RecurringExpense();
        a.setId(1L);
        RecurringExpense b = new RecurringExpense();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        RecurringExpense a = new RecurringExpense();
        a.setId(1L);
        RecurringExpense b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        RecurringExpense a = new RecurringExpense();
        a.setId(1L);
        RecurringExpense b = new RecurringExpense();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}