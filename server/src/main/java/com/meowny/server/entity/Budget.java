package com.meowny.server.entity;

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
                        columnNames = {"user_id", "category_id", "month", "year"}
                )
        },
        indexes = {
                @Index(name = "idx_budget_user_period", columnList = "user_id, year, month"),
                @Index(name = "idx_budget_user_category", columnList = "user_id, category_id")
        }
)
public class Budget extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @NotNull(message = "Budget must be assigned to a user")
    private User user;

    @Column(name = "amount_limit", nullable = false, precision = 12, scale = 2)
    @NotNull(message = "Limit amount is required")
    @PositiveOrZero(message = "Limit amount must be zero or positive")
    @Digits(integer = 12, fraction = 2)
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
    @JoinColumn(name = "category_id", nullable = false)
    @NotNull(message = "Category is required")
    private Category category;

    public Budget() {
    }

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }

    public User getUser() { return user; }
    public void setUser(User user) { this.user = user; }

    public BigDecimal getLimitAmount() { return limitAmount; }
    public void setLimitAmount(BigDecimal limitAmount) { this.limitAmount = limitAmount; }

    public Integer getMonth() { return month; }
    public void setMonth(Integer month) { this.month = month; }

    public Integer getYear() { return year; }
    public void setYear(Integer year) { this.year = year; }

    public Category getCategory() { return category; }
    public void setCategory(Category category) { this.category = category; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Budget budget = (Budget) o;
        return id != null && Objects.equals(id, budget.id);
    }

    @Override
    public int hashCode() { return getClass().hashCode(); }

    @Override
    public String toString() {
        return "Budget{id=" + id + ", month=" + month + ", year=" + year + ", limitAmount=" + limitAmount + '}';
    }
}