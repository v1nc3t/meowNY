package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.util.Objects;

@Entity
@Table(
        name = "budgets",
        uniqueConstraints = {
                @UniqueConstraint(
                        name = "unique_user_category_period",
                        columnNames = {"user_id", "expense_category_id", "month", "year"}
                )
        },
        indexes = {
                @Index(name = "idx_budget_user_period", columnList = "user_id, year, month"),
                @Index(name = "idx_budget_user_category", columnList = "user_id, expense_category_id")
        }
)
public class Budget {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @NotNull(message = "Budget must be assigned to a user")
    private User user;

    @Column(nullable = false, precision = 19, scale = 4)
    @NotNull(message = "Limit amount is required")
    @PositiveOrZero(message = "Limit amount must be zero or positive")
    @Digits(integer = 15, fraction = 4)
    private BigDecimal limitAmount;

    @Column(nullable = false)
    @NotNull(message = "Month is required")
    @Min(value = 1, message = "Month must be at least 1")
    @Max(value = 12, message = "Month must be at most 12")
    private Integer month;

    @Column(nullable = false)
    @NotNull(message = "Year is required")
    @Min(value = 2000, message = "Year must be 2000 or later")
    @Max(value = 9999, message = "Year must be 9999 or earlier")
    private Integer year;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "expense_category_id", nullable = false)
    @NotNull(message = "Category is required")
    private ExpenseCategory category;

    /**
     * Empty constructor for object mapper.
     */
    public Budget() {
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

    public BigDecimal getLimitAmount() {
        return limitAmount;
    }
    public void setLimitAmount(BigDecimal limitAmount) {
        this.limitAmount = limitAmount;
    }

    public Integer getMonth() {
        return month;
    }
    public void setMonth(Integer month) {
        this.month = month;
    }

    public Integer getYear() {
        return year;
    }
    public void setYear(Integer year) {
        this.year = year;
    }

    public ExpenseCategory getCategory() {
        return category;
    }
    public void setCategory(ExpenseCategory category) {
        this.category = category;
    }

    @PrePersist
    @PreUpdate
    private void validateOwnership() {
        if (category != null && category.getUser() != null) {
            if (!Objects.equals(category.getUser().getId(), user.getId())) {
                throw new IllegalStateException("Category does not belong to this user.");
            }
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        Budget budget = (Budget) o;
        return Objects.equals(user, budget.user)
                && Objects.equals(category, budget.category)
                && Objects.equals(month, budget.month)
                && Objects.equals(year, budget.year);
    }

    @Override
    public int hashCode() {
        return Objects.hash(user, category, month, year);
    }

    @Override
    public String toString() {
        return "Budget{id=" + id + ", month=" + month + ", year=" + year + ", limitAmount=" + limitAmount + '}';
    }
}
