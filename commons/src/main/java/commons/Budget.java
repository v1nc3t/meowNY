package commons;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;

import java.util.Objects;

@Entity
@Table(name = "budgets", uniqueConstraints = {
        @UniqueConstraint(
                name =  "unique_user_category_period",
                columnNames = {"user_id", "expense_category_id", "month", "year"}
        )
})
public class Budget {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id")
    private User user;

    @Column(nullable = false)
    @PositiveOrZero
    private Double limitAmount;

    @Column(nullable = false)
    @Min(1)
    @Max(12)
    private Integer month;

    @Column(nullable = false)
    @Min(1923)
    @Max(9999)
    private Integer year;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "expense_category_id", nullable = false)
    private ExpenseCategory category;

    /**
     * Empty constructor for object mapper
     */
    public Budget() {
    }

    public Budget(
            Double limitAmount,
            Integer month,
            Integer year,
            ExpenseCategory category,
            User user
    ) {
        if (limitAmount == null || limitAmount < 0) {
            throw new IllegalArgumentException("Limit amount must be positive.");
        }
        if (month == null || month < 1 || month > 12) {
            throw new IllegalArgumentException("Month must be between 1 and 12.");
        }
        if (year == null || year < 1923 || year > 9999) {
            throw  new IllegalArgumentException("Year must be between 1923 and 9999.");
        }
        if (category == null) {
            throw new IllegalArgumentException("Category must exist to create budget.");
        }
        if (user == null) {
            throw new IllegalArgumentException("Budget must be assigned to a user.");
        }
        this.limitAmount = limitAmount;
        this.month = month;
        this.year = year;
        this.category = category;
        this.user = user;
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
            throw new IllegalArgumentException("Budget must be assigned to a user.");
        }
        this.user = user;
    }

    public Double getLimitAmount() {
        return limitAmount;
    }

    public void setLimitAmount(Double limitAmount) {
        if (limitAmount == null || limitAmount < 0) {
            throw new IllegalArgumentException("Limit amount must be positive.");
        }
        this.limitAmount = limitAmount;
    }

    public Integer getMonth() {
        return month;
    }

    public void setMonth(Integer month) {
        if (month == null || month < 1 || month > 12) {
            throw new IllegalArgumentException("Month must be between 1 and 12.");
        }
        this.month = month;
    }

    public Integer getYear() {
        return year;
    }

    public void setYear(Integer year) {
        if (year == null || year < 1923 || year > 9999) {
            throw  new IllegalArgumentException("Year must be between 1923 and 9999.");
        }
        this.year = year;
    }

    public ExpenseCategory getCategory() {
        return category;
    }

    public void setCategory(ExpenseCategory category) {
        if (category == null) {
            throw new IllegalArgumentException("Category must exist to create budget.");
        }
        this.category = category;
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        Budget budget = (Budget) o;
        return Objects.equals(id, budget.id)
                && Objects.equals(user, budget.user)
                && Objects.equals(limitAmount, budget.limitAmount)
                && Objects.equals(month, budget.month)
                && Objects.equals(year, budget.year)
                && Objects.equals(category, budget.category);
    }

    @Override
    public int hashCode() {
        return Objects.hash(id, user, limitAmount, month, year, category);
    }

    @Override
    public String toString() {
        return "Budget{" +
                "id=" + id +
                ", user=" + user +
                ", limitAmount=" + limitAmount +
                ", month=" + month +
                ", year=" + year +
                ", category=" + category +
                '}';
    }
}
