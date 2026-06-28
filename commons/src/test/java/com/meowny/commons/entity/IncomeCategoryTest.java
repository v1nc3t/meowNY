package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import static org.assertj.core.api.Assertions.*;

public class IncomeCategoryTest {

    private User user;

    private IncomeCategory incomeCategory;

    @BeforeEach
    void setup() {
        user = new User();
        user.setId(1L);

        incomeCategory = new IncomeCategory();
    }

    @Test
    void shouldCreateWithValidFields() {
        incomeCategory.setUser(user);
        incomeCategory.setCategoryName("Test");

        assertThat(incomeCategory.getUser()).isEqualTo(user);
        assertThat(incomeCategory.getCategoryName()).isEqualTo("Test");
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        IncomeCategory a = new IncomeCategory();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        IncomeCategory a = new IncomeCategory();
        a.setId(1L);
        IncomeCategory b = new IncomeCategory();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        IncomeCategory a = new IncomeCategory();
        a.setId(1L);
        IncomeCategory b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        IncomeCategory a = new IncomeCategory();
        a.setId(1L);
        IncomeCategory b = new IncomeCategory();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}
