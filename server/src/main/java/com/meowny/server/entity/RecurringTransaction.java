package com.meowny.server.entity;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

import org.hibernate.annotations.OnDelete;
import org.hibernate.annotations.OnDeleteAction;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.FetchType;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.Index;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import jakarta.persistence.Table;
import jakarta.validation.constraints.Digits;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.PositiveOrZero;
import jakarta.validation.constraints.Size;

@Entity
@Table(name = "recurring_transactions",
        indexes = {
                @Index(name = "idx_recurring_tx_worker", columnList = "is_active, next_due_date"),
                @Index(name = "idx_recurring_tx_user", columnList = "user_id, is_active")
        }
)
public class RecurringTransaction extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @OnDelete(action = OnDeleteAction.CASCADE)
    @NotNull(message = "Template must be assigned to a user")
    private User user;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "category_id", nullable = false)
    @NotNull(message = "Category is required")
    private Category category;

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
    @NotNull(message = "Next due date is required")
    private LocalDate nextDueDate;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    @NotNull(message = "Frequency is required")
    private Frequency frequency;

    @Column(name = "is_active", nullable = false)
    private boolean isActive = true;

    @OneToMany(mappedBy = "sourceTemplate")
    private List<Transaction> history = new ArrayList<>();

    public RecurringTransaction() {}

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }
    public User getUser() { return user; }
    public void setUser(User user) { this.user = user; }
    public Category getCategory() { return category; }
    public void setCategory(Category category) { this.category = category; }
    public String getName() { return name; }
    public void setName(String name) { this.name = name; }
    public BigDecimal getAmount() { return amount; }
    public void setAmount(BigDecimal amount) { this.amount = amount; }
    public LocalDate getNextDueDate() { return nextDueDate; }
    public void setNextDueDate(LocalDate nextDueDate) { this.nextDueDate = nextDueDate; }
    public Frequency getFrequency() { return frequency; }
    public void setFrequency(Frequency frequency) { this.frequency = frequency; }
    public boolean isActive() { return isActive; }
    public void setActive(boolean active) { isActive = active; }
    public List<Transaction> getHistory() { return history; }

    public void addTransaction(Transaction tx) {
        this.history.add(tx);
        tx.setSourceTemplate(this);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        RecurringTransaction that = (RecurringTransaction) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() { return getClass().hashCode(); }
}
