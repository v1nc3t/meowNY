package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.NotBlank;

import java.util.Objects;

@Entity
@Table(
        name = "expense_categories",
        uniqueConstraints = {
                @UniqueConstraint(
                        name = "unique_user_expense_category_name",
                        columnNames = {"user_id", "category_name"}
                )
        },
        indexes = {
                @Index(name = "idx_expense_category_user", columnList = "user_id")
        }
)
public class ExpenseCategory {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @NotBlank
    @Column(name = "category_name", length = 30, nullable = false)
    private String categoryName;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "user_id", nullable = false)
    private User user;

    /**
     * Empty constructor for object mapper.
     */
    public ExpenseCategory() {
    }

    public ExpenseCategory(String categoryName, User user) {
        if (categoryName == null) {
            throw new IllegalArgumentException("Category name mustn't be null.");
        }
        if (user == null) {
            throw new IllegalArgumentException("Category must belong to a user.");
        }
        this.categoryName = categoryName;
        this.user = user;
    }

    public ExpenseCategory(String categoryName) {
        if (categoryName == null) {
            throw new IllegalArgumentException("Category name mustn't be null.");
        }
        this.categoryName = categoryName;
    }

    public Long getId() {
        return id;
    }

    public void setId(Long id) {
        this.id = id;
    }

    public String getCategoryName() {
        return categoryName;
    }

    public void setCategoryName(String categoryName) {
        if (categoryName == null) {
            throw new IllegalArgumentException("Category name mustn't be null.");
        }
        this.categoryName = categoryName;
    }

    public User getUser() {
        return user;
    }

    public void setUser(User user) {
        if (user == null) {
            throw new IllegalArgumentException("Category must belong to a user.");
        }
        this.user = user;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        ExpenseCategory that = (ExpenseCategory) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() {
        return Objects.hashCode(id);
    }

    @Override
    public String toString() {
        return "ExpenseCategory{" +
                "id=" + id +
                ", categoryName='" + categoryName + '\'' +
                '}';
    }
}
