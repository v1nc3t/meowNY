package com.meowny.server.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import org.hibernate.annotations.OnDelete;
import org.hibernate.annotations.OnDeleteAction;

import java.time.LocalDateTime;
import java.util.Objects;

@Entity
@Table(
        name = "categories",
        uniqueConstraints = {
                @UniqueConstraint(
                        name = "unique_user_category_name",
                        columnNames = {"user_id", "name"}
                )
        },
        indexes = {
                @Index(name = "idx_category_group", columnList = "category_group_id")
        }
)
public class Category extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    @OnDelete(action = OnDeleteAction.CASCADE)
    @NotNull(message = "Category must be assigned to a user")
    private User user;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "category_group_id")
    @OnDelete(action = OnDeleteAction.SET_NULL)
    private CategoryGroup categoryGroup;

    @Enumerated(EnumType.STRING)
    @Column(nullable = false, length = 10)
    @NotNull(message = "Category type is required")
    private TransactionType type;

    @Column(name = "name", length = 50, nullable = false)
    @NotBlank(message = "Category name is required")
    @Size(max = 50, message = "Category name must be 50 characters or fewer")
    private String name;

    @Column(name = "deleted_at")
    private LocalDateTime deletedAt;

    public Category() {}

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }

    public User getUser() { return user; }
    public void setUser(User user) { this.user = user; }

    public CategoryGroup getCategoryGroup() { return categoryGroup; }
    public void setCategoryGroup(CategoryGroup categoryGroup) { this.categoryGroup = categoryGroup; }

    public TransactionType getType() { return type; }
    public void setType(TransactionType type) { this.type = type; }

    public String getName() { return name; }
    public void setName(String name) { this.name = name; }

    public LocalDateTime getDeletedAt() { return deletedAt; }
    public void setDeletedAt(LocalDateTime deletedAt) { this.deletedAt = deletedAt; }

    public boolean isDeleted() {
        return deletedAt != null;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Category category = (Category) o;
        return id != null && Objects.equals(id, category.id);
    }

    @Override
    public int hashCode() { return getClass().hashCode(); }
}
