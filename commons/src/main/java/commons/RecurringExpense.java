package commons;

import jakarta.persistence.*;
import jakarta.validation.constraints.Min;

import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

@Entity
@Table(name = "recurring_expense", uniqueConstraints = {
        @UniqueConstraint(
                name = "unique_user_recurring_details",
                columnNames = {"user_id", "name", "amount", "frequency"}
        )
})
public class RecurringExpense {

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
    private LocalDate nextDueDate;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    private Frequency frequency;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "expense_category_id")
    private ExpenseCategory category;

    @OneToMany(mappedBy = "sourceTemplate", cascade = CascadeType.ALL, orphanRemoval = true)
    private List<Expense> paymentHistory = new ArrayList<>();

    /**
     * Empty constructor for object mapper
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
    }

    public ExpenseCategory getCategory() {
        return category;
    }

    public void setCategory(ExpenseCategory category) {
        if (category == null) {
            throw new IllegalArgumentException("Category mustn't be null.");
        }
        this.category = category;
    }

    public List<Expense> getPaymentHistory() {
        return paymentHistory;
    }

    public void setPaymentHistory(List<Expense> paymentHistory) {
        this.paymentHistory = paymentHistory;
    }

    public void addPayment(Expense expense) {
        this.paymentHistory.add(expense);
        expense.setSourceTemplate(this);
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        RecurringExpense that = (RecurringExpense) o;
        return Objects.equals(id, that.id)
                && Objects.equals(user, that.user)
                && Objects.equals(name, that.name)
                && Objects.equals(amount, that.amount)
                && Objects.equals(nextDueDate, that.nextDueDate)
                && frequency == that.frequency;
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                id,
                user,
                name,
                amount,
                nextDueDate,
                frequency
        );
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
