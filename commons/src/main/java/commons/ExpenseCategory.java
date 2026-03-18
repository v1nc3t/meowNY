package commons;

import jakarta.persistence.*;

import java.util.Objects;

@Entity
public class ExpenseCategory {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(length = 30, nullable = false)
    private String categoryName;

    /**
     * Empty constructor for object mapper
     */
    public ExpenseCategory() {
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

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        ExpenseCategory that = (ExpenseCategory) o;
        return Objects.equals(categoryName, that.categoryName);
    }

    @Override
    public int hashCode() {
        return Objects.hashCode(categoryName);
    }

    @Override
    public String toString() {
        return "commons.ExpenseCategory{" +
                "id=" + id +
                ", categoryName='" + categoryName + '\'' +
                '}';
    }
}
