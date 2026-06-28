package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import static org.assertj.core.api.Assertions.*;


public class ExpenseCategoryTest {

    private User user;
    private ExpenseCategory expenseCategory;

    @BeforeEach
    void setup() {
        user = new User();
        user.setId(1L);

        expenseCategory = new ExpenseCategory();
        expenseCategory.setId(1L);
    }

    @Test
    void shouldCreateWithValidFields() {
        expenseCategory.setUser(user);
        expenseCategory.setCategoryName("Test");

        assertThat(expenseCategory.getCategoryName()).isEqualTo("Test");
        assertThat(expenseCategory.getUser()).isEqualTo(user);
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        ExpenseCategory a = new ExpenseCategory();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        ExpenseCategory a = new ExpenseCategory();
        a.setId(1L);
        ExpenseCategory b = new ExpenseCategory();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        ExpenseCategory a = new ExpenseCategory();
        a.setId(1L);
        Expense b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldBeEqualWithDifferentId() {
        ExpenseCategory a = new ExpenseCategory();
        a.setId(1L);
        ExpenseCategory b = new ExpenseCategory();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}
