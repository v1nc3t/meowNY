package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import java.math.BigDecimal;

import static org.assertj.core.api.Assertions.*;

public class IncomeTest {

    private User user;
    private IncomeCategory incomeCategory;

    private Income income;

    @BeforeEach
    void setup() {
        user = new User();
        user.setId(1L);

        incomeCategory = new IncomeCategory();
        incomeCategory.setId(1L);

        income = new Income();
    }

    @Test
    void shouldCreateWithValidFields() {
        income.setUser(user);
        income.setName("Test");
        income.setAmount(new BigDecimal("1.00"));
        income.setCategory(incomeCategory);

        assertThat(income.getUser()).isEqualTo(user);
        assertThat(income.getName()).isEqualTo("Test");
        assertThat(income.getAmount()).isEqualByComparingTo("1");
        assertThat(income.getCategory()).isEqualTo(incomeCategory);
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        Income a = new Income();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        Income a = new Income();
        a.setId(1L);
        Income b = new Income();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        Income a = new Income();
        a.setId(1L);
        Income b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        Income a = new Income();
        a.setId(1L);
        Income b = new Income();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}

