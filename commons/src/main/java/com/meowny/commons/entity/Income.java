package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.Objects;

@Entity
@Table(
        name = "income",
        indexes = {
                @Index(name = "idx_income_category_user", columnList = "income_category_id, user_id"),
                @Index(name = "idx_income_user_date", columnList = "user_id, payment_date")
        }
)
public class Income extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id")
    @NotNull(message = "Income must be assigned to a user")
    private User user;

    @Column(length = 50, nullable = false)
    @NotBlank(message = "Name is required")
    @Size(max = 50, message = "Name must be 50 characters or fewer")
    private String name;

    @Column(nullable = false, precision = 12, scale = 2)
    @NotNull(message = "Amount is required")
    @PositiveOrZero(message = "Amount must be zero or positive")
    @Digits(integer = 12, fraction = 2)
    private BigDecimal amount;

    @Column(name = "payment_date", nullable = false)
    @NotNull(message = "Payment date is required")
    @PastOrPresent(message = "Payment date cannot be in the future")
    private LocalDate paymentDate;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "recurring_income_id")
    private RecurringIncome sourceTemplate;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumns({
            @JoinColumn(name = "income_category_id", referencedColumnName = "id", nullable = false),
            @JoinColumn(name = "user_id", referencedColumnName = "user_id", nullable = false, insertable = false, updatable = false)
    })
    @NotNull(message = "Income category is required")
    private IncomeCategory category;

    public Income() {
    }

    public Long getId() {
        return id;
    }
    public void setId(Long id) {
        this.id = id;
    }

    public User getUser() {
        return user;
    }
    public void setUser(User user) {
        this.user = user;
    }

    public String getName() {
        return name;
    }
    public void setName(String name) {
        this.name = name;
    }

    public BigDecimal getAmount() {
        return amount;
    }
    public void setAmount(BigDecimal amount) {
        this.amount = amount;
    }

    public LocalDate getPaymentDate() { return paymentDate; }
    public void setPaymentDate(LocalDate paymentDate) { this.paymentDate = paymentDate; }

    public RecurringIncome getSourceTemplate() { return sourceTemplate; }
    public void setSourceTemplate(RecurringIncome sourceTemplate) { this.sourceTemplate = sourceTemplate; }

    public IncomeCategory getCategory() {
        return category;
    }
    public void setCategory(IncomeCategory category) {
        this.category = category;
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        Income income = (Income) o;
        return id != null && Objects.equals(id, income.id);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    @Override
    public String toString() {
        return "Income{" +
                "id=" + id +
                ", name='" + name + '\'' +
                ", amount=" + amount +
                ", paymentDate=" + paymentDate +
                '}';
    }
}
