package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

@Entity
@Table(name = "recurring_expense",
        uniqueConstraints = {
            @UniqueConstraint(
                name = "unique_user_recurring_details",
                columnNames = {"user_id", "name", "amount", "frequency"}
            )
        },
        indexes = {
            @Index(name = "idx_recurring_user_due_date", columnList = "user_id, next_due_date"),
            @Index(name = "idx_recurring_user_frequency_due", columnList = "user_id, frequency, next_due_date"),
            @Index(name = "idx_recurring_user_category", columnList = "user_id, expense_category_id")
        }
)
public class RecurringExpense extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @NotNull(message = "Recurring expense must be assigend to a user")
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

    @Column(name = "next_due_date", nullable = false)
    @NotNull(message = "Next due daate is required")
    @FutureOrPresent
    private LocalDate nextDueDate;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    @NotNull(message = "Frequency is required")
    private Frequency frequency;

    @Column(name = "is_active", nullable = false)
    private boolean isActive = true;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumns({
            @JoinColumn(name = "expense_category_id", referencedColumnName = "id", nullable = false),
            @JoinColumn(name = "user_id", referencedColumnName = "user_id", nullable = false, insertable = false, updatable = false)
    })
    @NotNull(message = "Expense category is required")
    private ExpenseCategory category;

    @OneToMany(mappedBy = "sourceTemplate")
    private List<Expense> paymentHistory = new ArrayList<>();

    public RecurringExpense() {
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

    public LocalDate getNextDueDate() {
        return nextDueDate;
    }
    public void setNextDueDate(LocalDate nextDueDate) {
        this.nextDueDate = nextDueDate;
    }

    public Frequency getFrequency() {
        return frequency;
    }
    public void setFrequency(Frequency frequency) {
        this.frequency = frequency;
    }

    public boolean isActive() { return isActive; }
    public void setActive(boolean active) { isActive = active; }

    public ExpenseCategory getCategory() {
        return category;
    }
    public void setCategory(ExpenseCategory category) {
        this.category = category;
    }

    public List<Expense> getPaymentHistory() {
        return paymentHistory;
    }
    public void setPaymentHistory(List<Expense> paymentHistory) {
        this.paymentHistory.clear();
        if (paymentHistory != null) {
            paymentHistory.forEach(this::addPayment);
        }
    }

    public void addPayment(Expense expense) {
        if (expense == null) {
            throw new IllegalArgumentException("Expense payment mustn't be null.");
        }
        this.paymentHistory.add(expense);
        expense.setSourceTemplate(this);
    }
    public void removePayment(Expense expense) {
        if (expense == null) return;
        this.paymentHistory.remove(expense);
        if (expense.getSourceTemplate() == this) {
            expense.setSourceTemplate(null);
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        RecurringExpense that = (RecurringExpense) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    @Override
    public String toString() {
        return "RecurringExpense{" +
                "id=" + id +
                ", name='" + name + '\'' +
                ", amount=" + amount +
                ", nextDueDate=" + nextDueDate +
                ", frequency=" + frequency +
                ", isActive=" + isActive +
                '}';
    }
}
