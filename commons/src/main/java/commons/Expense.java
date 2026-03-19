package commons;

import jakarta.persistence.*;
import jakarta.validation.constraints.Min;

import java.time.LocalDate;
import java.util.Objects;

@Entity
public class Expense {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    private User user;

    @Column(length = 30, nullable = false)
    private String name;

    @Column(nullable = false)
    @Min(0)
    private Double amount;

    @Column(nullable = false)
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
        this.category = category;
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        Expense expense = (Expense) o;
        return Objects.equals(id, expense.id)
                && Objects.equals(user, expense.user)
                && Objects.equals(name, expense.name)
                && Objects.equals(amount, expense.amount)
                && Objects.equals(paymentDate, expense.paymentDate)
                && Objects.equals(referenceDueDate, expense.referenceDueDate)
                && Objects.equals(description, expense.description)
                && Objects.equals(sourceTemplate, expense.sourceTemplate)
                && Objects.equals(category, expense.category);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                id,
                user,
                name,
                amount,
                paymentDate,
                referenceDueDate,
                description,
                sourceTemplate,
                category
        );
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
