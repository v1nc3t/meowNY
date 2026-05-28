package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.PositiveOrZero;

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
public class RecurringExpense {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    private User user;

    @NotBlank
    @Column(length = 30, nullable = false)
    private String name;

    @PositiveOrZero
    @Column(nullable = false)
    private Double amount;

    @Column(name = "next_due_date", nullable = false)
    private LocalDate nextDueDate;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    private Frequency frequency;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "expense_category_id", nullable = false)
    private ExpenseCategory category;

    @OneToMany(mappedBy = "sourceTemplate", cascade = CascadeType.ALL, orphanRemoval = true)
    private List<Expense> paymentHistory = new ArrayList<>();

    /**
     * Empty constructor for object mapper.
     */
    public RecurringExpense() {
    }

    public RecurringExpense(
            String name,
            Double amount,
            LocalDate nextDueDate,
            Frequency frequency,
            User user,
            ExpenseCategory category
    ) {
        if (name == null) {
            throw new IllegalArgumentException("Recurring name mustn't be null.");
        }
        if (amount == null || amount < 0.0) {
            throw new IllegalArgumentException("Amount must be positive.");
        }
        if (nextDueDate == null) {
            throw new IllegalArgumentException("Next due date mustn't be null.");
        }
        if (nextDueDate.isBefore(LocalDate.now())) {
            throw new IllegalArgumentException("Next due date cannot be in the past.");
        }
        if (frequency == null) {
            throw new IllegalArgumentException("Frequency mustn't be null.");
        }
        if (category == null) {
            throw new IllegalArgumentException("Category mustn't be null.");
        }
        if (user == null) {
            throw new IllegalArgumentException("User mustn't be null.");
        }
        this.name = name;
        this.amount = amount;
        this.nextDueDate = nextDueDate;
        this.frequency = frequency;
        this.user = user;
        this.category = category;
        validateCategoryBelongsToUser(this.category, this.user);
    }

    public Long getId() {
        return id;
    }

    public void setId(Long id) {
        this.id = id;
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        if (name == null) {
            throw new IllegalArgumentException("Recurring name mustn't be null.");
        }
        this.name = name;
    }

    public Double getAmount() {
        return amount;
    }

    public void setAmount(Double amount) {
        if (amount == null || amount < 0.0) {
            throw new IllegalArgumentException("Amount must be positive.");
        }
        this.amount = amount;
    }

    public LocalDate getNextDueDate() {
        return nextDueDate;
    }

    public void setNextDueDate(LocalDate nextDueDate) {
        if (nextDueDate == null) {
            throw new IllegalArgumentException("Next due date mustn't be null.");
        }
        if (nextDueDate.isBefore(LocalDate.now())) {
            throw new IllegalArgumentException("Next due date cannot be in the past.");
        }
        this.nextDueDate = nextDueDate;
    }

    public Frequency getFrequency() {
        return frequency;
    }

    public void setFrequency(Frequency frequency) {
        if (frequency == null) {
            throw new IllegalArgumentException("Frequency mustn't be null.");
        }
        this.frequency = frequency;
    }

    public User getUser() {
        return user;
    }

    public void setUser(User user) {
        if (user == null) {
            throw new IllegalArgumentException("User mustn't be null.");
        }
        this.user = user;
        validateCategoryBelongsToUser(this.category, user);
    }

    public ExpenseCategory getCategory() {
        return category;
    }

    public void setCategory(ExpenseCategory category) {
        if (category == null) {
            throw new IllegalArgumentException("Category mustn't be null.");
        }
        validateCategoryBelongsToUser(category, this.user);
        this.category = category;
    }

    public List<Expense> getPaymentHistory() {
        return paymentHistory;
    }

    public void setPaymentHistory(List<Expense> paymentHistory) {
        this.paymentHistory.clear();
        if (paymentHistory == null) {
            return;
        }
        for (Expense expense : paymentHistory) {
            addPayment(expense);
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
        if (expense == null) {
            return;
        }
        this.paymentHistory.remove(expense);
        if (expense.getSourceTemplate() == this) {
            expense.setSourceTemplate(null);
        }
    }

    private void validateCategoryBelongsToUser(ExpenseCategory category, User user) {
        if (category == null || user == null) {
            return;
        }
        User categoryUser = category.getUser();
        if (categoryUser != null && !Objects.equals(categoryUser.getId(), user.getId())) {
            throw new IllegalArgumentException("Category must belong to the same user as the recurring expense.");
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
        return Objects.hashCode(id);
    }

    @Override
    public String toString() {
        return "RecurringExpense{" +
                "id=" + id +
                ", user=" + user +
                ", recurringName='" + name + '\'' +
                ", amount=" + amount +
                ", nextDueDate=" + nextDueDate +
                ", frequency=" + frequency +
                '}';
    }
}
