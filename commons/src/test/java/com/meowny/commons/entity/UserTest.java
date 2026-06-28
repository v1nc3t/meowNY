package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import java.util.List;

import static org.assertj.core.api.Assertions.*;

public class UserTest {

    private User user;

    @BeforeEach
    void setup() {
         user = new User();
         user.setId(1L);
    }

    @Test
    void shouldCreateWithValidFields() {
        user.setFirstName("Test");
        user.setLastName("Names");
        user.setEmail("g@mail.c");
        user.setUsername("TesNa");
        user.setPassword("Passs");

        assertThat(user.getFirstName()).isEqualTo("Test");
        assertThat(user.getLastName()).isEqualTo("Names");
        assertThat(user.getFullName()).isEqualTo("Test" + " " + "Names");
        assertThat(user.getEmail()).isEqualTo("g@mail.c");
        assertThat(user.getUsername()).isEqualTo("TesNa");
        assertThat(user.getPassword()).isEqualTo("Passs");
    }

    @Test
    void shouldAddRemoveIncomes() {
        Income income = new Income();
        income.setId(1L);

        user.addIncome(income);
        assertThat(user.getIncomes()).containsExactlyElementsOf(List.of(income));

        assertThatThrownBy(() -> user.addIncome(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("mustn't be null");

        user.removeIncome(null);
        assertThat(user.getIncomes()).containsExactlyElementsOf(List.of(income));

        user.removeIncome(income);
        assertThat(user.getIncomes()).containsExactlyElementsOf(List.of());
    }

    @Test
    void shouldAddRemoveExpenses() {
        Expense expense = new Expense();
        expense.setId(1L);

        user.addExpense(expense);
        assertThat(user.getExpenses()).containsExactlyElementsOf(List.of(expense));

        assertThatThrownBy(() -> user.addExpense(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("mustn't be null");

        user.removeExpense(null);
        assertThat(user.getExpenses()).containsExactlyElementsOf(List.of(expense));

        user.removeExpense(expense);
        assertThat(user.getExpenses()).containsExactlyElementsOf(List.of());
    }

    @Test
    void sholdAddRemoveBudgets() {
        Budget budget = new Budget();
        budget.setId(1L);

        user.addBudget(budget);
        assertThat(user.getBudgets()).containsExactlyElementsOf(List.of(budget));

        assertThatThrownBy(() -> user.addBudget(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("mustn't be null");

        user.removeBudget(null);
        assertThat(user.getBudgets()).containsExactlyElementsOf(List.of(budget));

        user.removeBudget(budget);
        assertThat(user.getBudgets()).containsExactlyElementsOf(List.of());
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        User a = new User();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }


    @Test
    void shouldBeEqualWithSameId() {
        User a = new User();
        a.setId(1L);
        User b = new User();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        User a = new User();
        a.setId(1L);
        User b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        User a = new User();
        a.setId(1L);
        User b = new User();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

}
