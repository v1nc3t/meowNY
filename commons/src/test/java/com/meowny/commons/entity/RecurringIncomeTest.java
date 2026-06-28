package com.meowny.commons.entity;

import org.junit.jupiter.api.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;

import static org.assertj.core.api.Assertions.*;

public class RecurringIncomeTest {

    private User user;
    private IncomeCategory incomeCategory;
    private LocalDate testDate = LocalDate.of(2020, 1, 1);

    private RecurringIncome recurringIncome;

    @BeforeEach
    void setup() {
        user = new User();
        user.setId(1L);

        incomeCategory = new IncomeCategory();
        incomeCategory.setId(1L);
        incomeCategory.setUser(user);

        recurringIncome = new RecurringIncome();
    }

    @Test
    void shouldCreateWithValidFields() {
        recurringIncome.setUser(user);
        recurringIncome.setName("Test");
        recurringIncome.setAmount(new BigDecimal("1.00"));
        recurringIncome.setNextDepositDate(testDate);
        recurringIncome.setFrequency(Frequency.DAILY);
        recurringIncome.setActive(true);
        recurringIncome.setCategory(incomeCategory);

        assertThat(recurringIncome.getUser()).isEqualTo(user);
        assertThat(recurringIncome.getName()).isEqualTo("Test");
        assertThat(recurringIncome.getAmount()).isEqualByComparingTo("1.00");
        assertThat(recurringIncome.getNextDepositDate()).isEqualTo(testDate);
        assertThat(recurringIncome.getFrequency()).isEqualTo(Frequency.DAILY);
        assertThat(recurringIncome.isActive()).isTrue();
        assertThat(recurringIncome.getCategory()).isEqualTo(incomeCategory);
    }

    @Test
    void shouldAllowZeroLimitAmount() {
        assertThatNoException().isThrownBy(() -> recurringIncome.setAmount(BigDecimal.ZERO));
    }

    @Test
    void shouldAddRemovePaymentHistory() {
        Income e10 = new Income();
        e10.setId(10L);

        recurringIncome.setIncomeHistory(new ArrayList<>());
        recurringIncome.addPayment(e10);
        assertThat(recurringIncome.getIncomeHistory())
                .containsExactlyElementsOf(List.of(e10));
    }

    @Test
    void shouldNotAddNull() {
        assertThatThrownBy(() -> recurringIncome.addPayment(null))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("payment mustn't be null");
    }

    @Test
    void shouldRemovePaymentHistory() {
        Income e11 = new Income();
        e11.setId(11L);

        recurringIncome.addPayment(e11);
        recurringIncome.removePayment(e11);

        assertThat(recurringIncome.getIncomeHistory())
                .containsExactlyElementsOf(List.of());
    }

    @Test
    void shouldRemoveSourceTempalte() {
        Income e11 = new Income();
        e11.setId(11L);

        e11.setSourceTemplate(recurringIncome);

        recurringIncome.addPayment(e11);
        recurringIncome.removePayment(e11);

        assertThat(recurringIncome.getIncomeHistory())
                .containsExactlyElementsOf(List.of());
        assertThat(e11.getSourceTemplate()).isNull();
    }

    @Test
    void shouldDoNothingRemoveNull() {
        Income e11 = new Income();
        e11.setId(11L);

        recurringIncome.addPayment(e11);
        recurringIncome.removePayment(null);

        assertThat(recurringIncome.getIncomeHistory())
                .containsExactlyElementsOf(List.of(e11));
        assertThat(e11.getSourceTemplate()).isNotNull();
    }

    // equals / hashcode

    @Test
    void shouldEqualSame() {
        RecurringIncome a = new RecurringIncome();
        a.setId(1L);

        assertThat(a).isEqualTo(a);
        assertThat(a.hashCode()).isEqualTo(a.hashCode());
    }

    @Test
    void shouldBeEqualWithSameId() {
        RecurringIncome a = new RecurringIncome();
        a.setId(1L);
        RecurringIncome b = new RecurringIncome();
        b.setId(1L);

        assertThat(a).isEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }

    @Test
    void shouldNotBeEqualNull() {
        RecurringIncome a = new RecurringIncome();
        a.setId(1L);
        RecurringIncome b = null;

        assertThat(a).isNotEqualTo(b);
    }

    @Test
    void shouldNotBeEqualWithDifferentId() {
        RecurringIncome a = new RecurringIncome();
        a.setId(1L);
        RecurringIncome b = new RecurringIncome();
        b.setId(2L);

        assertThat(a).isNotEqualTo(b);
        assertThat(a.hashCode()).isEqualTo(b.hashCode());
    }
}
