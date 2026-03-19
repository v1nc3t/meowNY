package commons;

import jakarta.persistence.*;
import jakarta.validation.constraints.Min;

import java.util.Objects;

@Entity
public class Income {
    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id")
    private User user;

    @Column(length = 30, nullable = false)
    private String name;

    @Column(nullable = false)
    @Min(0)
    private Double amount;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "income_category_id")
    private IncomeCategory category;

    /**
     * Empty constructor for object mapper
     */
    public Income() {
    }

    public Income(
            String name,
            Double amount,
            IncomeCategory category,
            User user
    ) {
        if (name == null) {
            throw new IllegalArgumentException("Name mustn't be null.");
        }
        if (amount == null || amount <= 0.0) {
            throw new IllegalArgumentException("Amount must be positive.");
        }
        if (category == null) {
            throw new IllegalArgumentException("Category mustn't be null.");
        }
        if (user == null) {
            throw new IllegalArgumentException("Income must be assigned to a user.");
        }
        this.name = name;
        this.amount = amount;
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
            throw new IllegalArgumentException("Income must be assigned to a user.");
        }
        this.user = user;
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        if (name == null) {
            throw new IllegalArgumentException("Name mustn't be null.");
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

    public IncomeCategory getCategory() {
        return category;
    }

    public void setCategory(IncomeCategory category) {
        if (category == null) {
            throw new IllegalArgumentException("Category mustn't be null.");
        }
        this.category = category;
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        Income income = (Income) o;
        return Objects.equals(id, income.id)
                && Objects.equals(user, income.user)
                && Objects.equals(name, income.name)
                && Objects.equals(amount, income.amount)
                && Objects.equals(category, income.category);
    }

    @Override
    public int hashCode() {
        return Objects.hash(id, user, name, amount, category);
    }

    @Override
    public String toString() {
        return "Income{" +
                "id=" + id +
                ", user=" + user +
                ", name='" + name + '\'' +
                ", amount=" + amount +
                ", category=" + category +
                '}';
    }
}
