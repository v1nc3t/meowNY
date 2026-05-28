package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Positive;

import java.time.LocalDate;
import java.util.Objects;

@Entity
@Table(
        name = "expenses",
        indexes = {
                @Index(name = "idx_expense_user_date", columnList = "user_id, payment_date"),
                @Index(name = "idx_expense_user_category", columnList = "user_id, expense_category_id"),
                @Index(name = "idx_expense_recurring_template", columnList = "recurring_expense_id")
        }
)
public class Expense {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    private User user;

    @NotBlank
    @Column(length = 30, nullable = false)
    private String name;

    @Column(nullable = false)
    @Positive
    private Double amount;

    @Column(name = "payment_date", nullable = false)
    private LocalDate paymentDate;

    private LocalDate referenceDueDate;

    @Column(length = 50)
    private String description;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "recurring_expense_id")
    private RecurringExpense sourceTemplate;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "expense_category_id", nullable = false)
    private ExpenseCategory category;


    /**
     * Empty constructor for object mapper.
     */
    public Expense() {
    }

    public Expense (
            String name,
            Double amount,
            LocalDate paymentDate,
            LocalDate referenceDueDate,
            String description,
            ExpenseCategory category,
            User user
    ) {
        if (name == null) {
            throw new IllegalArgumentException("Name can not be null.");
        }
        if (amount == null || amount <= 0.0) {
            throw new IllegalArgumentException("Amount must be positive.");
        }
        if (paymentDate == null) {
            throw new IllegalArgumentException("Payment date cannot be null.");
        }
        if (category == null) {
            throw new IllegalArgumentException("Category can not be null.");
        }
        if (user == null) {
            throw new IllegalArgumentException("User cannot be null.");
        }
        this.name = name;
        this.amount = amount;
        this.paymentDate = paymentDate;
        this.referenceDueDate = referenceDueDate;
        this.description = description;
        this.category = category;
        this.user = user;
        validateCategoryBelongsToUser(this.category, this.user);
    }

    public Expense(
            String name,
            Double amount,
            LocalDate paymentDate,
            String description,
            ExpenseCategory category,
            User user
    ) {
        this(name, amount, paymentDate, null, description, category, user);
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
        if (user == null) {
            throw new IllegalArgumentException("User cannot be null.");
        }
        this.user = user;
        validateCategoryBelongsToUser(this.category, user);
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        if (name == null) {
            throw new IllegalArgumentException("Name can not be null.");
        }
        this.name = name;
    }

    public Double getAmount() {
        return amount;
    }

    public void setAmount(Double amount) {
        if (amount == null || amount <= 0.0) {
            throw new IllegalArgumentException("Amount must be positive.");
        }
        this.amount = amount;
    }

    public LocalDate getPaymentDate() {
        return paymentDate;
    }

    public void setPaymentDate(LocalDate paymentDate) {
        if (paymentDate == null) {
            throw new IllegalArgumentException("Payment date cannot be null.");
        }
        this.paymentDate = paymentDate;
    }

    public LocalDate getReferenceDueDate() {
        return referenceDueDate;
    }

    public void setReferenceDueDate(LocalDate referenceDueDate) {
        this.referenceDueDate = referenceDueDate;
    }

    public String getDescription() {
        return description;
    }

    public void setDescription(String description) {
        this.description = description;
    }

    public RecurringExpense getSourceTemplate() {
        return sourceTemplate;
    }

    public void setSourceTemplate(RecurringExpense sourceTemplate) {
        this.sourceTemplate = sourceTemplate;
    }

    public ExpenseCategory getCategory() {
        return category;
    }

    public void setCategory(ExpenseCategory category) {
        if (category == null) {
            throw new IllegalArgumentException("Category can not be null.");
        }
        validateCategoryBelongsToUser(category, this.user);
        this.category = category;
    }

    private void validateCategoryBelongsToUser(ExpenseCategory category, User user) {
        if (category == null || user == null) {
            return;
        }
        User categoryUser = category.getUser();
        if (categoryUser != null && !Objects.equals(categoryUser.getId(), user.getId())) {
            throw new IllegalArgumentException("Category must belong to the same user as the expense.");
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Expense expense = (Expense) o;
        return id != null && Objects.equals(id, expense.id);
    }

    @Override
    public int hashCode() {
        return Objects.hashCode(id);
    }

    @Override
    public String toString() {
        return "Expense{" +
                "id=" + id +
                ", user=" + user +
                ", name='" + name + '\'' +
                ", amount=" + amount +
                ", paymentDate=" + paymentDate +
                ", referenceDueDate=" + referenceDueDate +
                ", description='" + description + '\'' +
                ", sourceTemplate=" + sourceTemplate +
                ", category=" + category +
                '}';
    }
}
