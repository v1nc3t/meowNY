package com.meowny.server.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;
import org.hibernate.annotations.OnDelete;
import org.hibernate.annotations.OnDeleteAction;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.Objects;

@Entity
@Table(
        name = "transactions",
        indexes = {
                @Index(name = "idx_tx_user_date", columnList = "user_id, payment_date DESC"),
                @Index(name = "idx_tx_user_category_date", columnList = "user_id, category_id, payment_date DESC"),
                @Index(name = "idx_tx_recurring", columnList = "recurring_transaction_id")
        }
)
public class Transaction extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @OnDelete(action = OnDeleteAction.CASCADE)
    @NotNull(message = "Transaction must be assigned to a user")
    private User user;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "category_id", nullable = false)
    @NotNull(message = "Category is required")
    private Category category;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "recurring_transaction_id")
    @OnDelete(action = OnDeleteAction.SET_NULL)
    private RecurringTransaction sourceTemplate;

    @Column(length = 50, nullable = false)
    @NotBlank(message = "Name is required")
    @Size(max = 50, message = "Name must be 50 characters or fewer")
    private String name;

    @NotNull(message = "Amount is required")
    @Column(nullable = false, precision = 12, scale = 2)
    @PositiveOrZero(message = "Amount must be zero or positive")
    @Digits(integer = 12, fraction = 2)
    private BigDecimal amount;

    @Column(name = "payment_date", nullable = false)
    @NotNull(message = "Payment date is required")
    @PastOrPresent(message = "Payment date cannot be in the future")
    private LocalDate paymentDate;

    @Column(length = 255)
    @Size(max = 255, message = "Description must be 255 characters or fewer")
    private String description;

    public Transaction() {}

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }

    public User getUser() { return user; }
    public void setUser(User user) { this.user = user; }

    public Category getCategory() { return category; }
    public void setCategory(Category category) { this.category = category; }

    public RecurringTransaction getSourceTemplate() { return sourceTemplate; }
    public void setSourceTemplate(RecurringTransaction sourceTemplate) { this.sourceTemplate = sourceTemplate; }

    public String getName() { return name; }
    public void setName(String name) { this.name = name; }

    public BigDecimal getAmount() { return amount; }
    public void setAmount(BigDecimal amount) { this.amount = amount; }

    public LocalDate getPaymentDate() { return paymentDate; }
    public void setPaymentDate(LocalDate paymentDate) { this.paymentDate = paymentDate; }

    public String getDescription() { return description; }
    public void setDescription(String description) { this.description = description; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Transaction that = (Transaction) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() { return getClass().hashCode(); }
}
