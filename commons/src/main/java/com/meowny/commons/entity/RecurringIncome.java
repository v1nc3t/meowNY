package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

@Entity
@Table(
        name = "recurring_income",
        indexes = {
                @Index(name = "idx_recurring_income_user", columnList = "user_id"),
                @Index(name = "idx_recurring_income_status_deposit", columnList = "is_active, next_deposit_date"), // Fixed
                @Index(name = "idx_recurring_income_category_user", columnList = "income_category_id, user_id")
        }
)
public class RecurringIncome extends BaseAuditEntity {

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

    @Column(name = "next_deposit_date", nullable = false)
    @NotNull(message = "Next deposit daate is required")
    @FutureOrPresent
    private LocalDate nextDepositDate;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    @NotNull(message = "Frequency is required")
    private Frequency frequency;

    @Column(name = "is_active", nullable = false)
    private boolean isActive = true;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumns({
            @JoinColumn(name = "income_category_id", referencedColumnName = "id", nullable = false),
            @JoinColumn(name = "user_id", referencedColumnName = "user_id", nullable = false, insertable = false, updatable = false)
    })
    @NotNull(message = "Income category is required")
    private IncomeCategory category;

    @OneToMany(mappedBy = "sourceTemplate")
    private List<Income> incomeHistory = new ArrayList<>();

    public RecurringIncome() {
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

    public LocalDate getNextDepositDate() {
        return nextDepositDate;
    }
    public void setNextDepositDate(LocalDate nextDepositDate) {
        this.nextDepositDate = nextDepositDate;
    }

    public Frequency getFrequency() {
        return frequency;
    }
    public void setFrequency(Frequency frequency) {
        this.frequency = frequency;
    }

    public boolean isActive() {
        return isActive;
    }
    public void setActive(boolean active) {
        isActive = active;
    }

    public IncomeCategory getCategory() {
        return category;
    }
    public void setCategory(IncomeCategory category) {
        this.category = category;
    }

    public List<Income> getIncomeHistory() {
        return incomeHistory;
    }
    public void setIncomeHistory(List<Income> incomeHistory) {
        this.incomeHistory.clear();
        if (incomeHistory != null) {
            incomeHistory.forEach(this::addPayment);
        }
    }

    public void addPayment(Income income) {
        if (income == null) {
            throw new IllegalArgumentException("Income payment mustn't be null.");
        }
        this.incomeHistory.add(income);
        income.setSourceTemplate(this);
    }
    public void removePayment(Income income) {
        if (income == null) return;
        this.incomeHistory.remove(income);
        if (income.getSourceTemplate() == this) {
            income.setSourceTemplate(null);
        }
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        RecurringIncome that = (RecurringIncome) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    @Override
    public String toString() {
        return "RecurringIncome{" +
                "id=" + id +
                ", name='" + name + '\'' +
                ", amount=" + amount +
                ", nextDueDate=" + nextDepositDate +
                ", frequency=" + frequency +
                ", isActive=" + isActive +
                '}';
    }
}
