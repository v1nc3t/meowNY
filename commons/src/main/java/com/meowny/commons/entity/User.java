package com.meowny.commons.entity;

import jakarta.persistence.*;
import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

@Entity
@Table(name = "users")
public class User extends BaseAuditEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(name = "firstname", length = 50, nullable = false)
    @NotBlank(message = "First name is required")
    @Size(max = 50, message = "First name must be 50 characters or fewer")
    private String firstName;

    @Column(name = "lastname", length = 30, nullable = false)
    @NotBlank(message = "Last nmae is required")
    @Size(max = 30, message = "Last name must be 30 characters or fewer")
    private String lastName;

    @Email(message = "Must be a valid email format")
    @Column(length = 254, nullable = false, unique = true)
    @NotBlank(message = "Email is required")
    @Size(max = 254, message = "Email must be 254 characters or fewer")
    private String email;

    @Column(length = 30, nullable = false)
    @NotBlank(message = "Username is required")
    @Size(max = 30, message = "Usename must be 30 characters or fewer")
    private String username;

    @Column(length = 100, nullable = false)
    @NotBlank(message = "Password is required")
    @Size(max = 100, message = "Password must be 100 characters or fewer")
    private String password;

    @OneToMany(mappedBy = "user", cascade = CascadeType.ALL, orphanRemoval = true)
    private List<Income> incomes = new ArrayList<>();

    @OneToMany(mappedBy = "user", cascade = CascadeType.ALL, orphanRemoval = true)
    private List<Expense> expenses = new ArrayList<>();

    @OneToMany(mappedBy = "user", cascade = CascadeType.ALL, orphanRemoval = true)
    private List<Budget> budgets = new ArrayList<>();

    /**
     * Empty constructor for object mapper.
     */
    public User() {
    }

    public Long getId() {
        return id;
    }
    public void setId(Long id) {
        this.id = id;
    }

    public String getFirstName() {
        return firstName;
    }
    public void setFirstName(String firstName) {
        this.firstName = firstName;
    }

    public String getLastName() {
        return lastName;
    }
    public void setLastName(String lastName) {
        this.lastName = lastName;
    }

    public String getFullName() {
        return this.firstName + " " + this.lastName;
    }

    public String getEmail() {
        return email;
    }
    public void setEmail(String email) {
        this.email = email;
    }

    public String getUsername() {
        return username;
    }
    public void setUsername(String username) {
        this.username = username;
    }

    public String getPassword() {
        return password;
    }
    public void setPassword(String password) {
        this.password = password;
    }

    public List<Income> getIncomes() {
        return incomes;
    }
    public void setIncomes(List<Income> incomes) {
        this.incomes = incomes;
    }

    public void addIncome(Income income) {
        if (income == null) {
            throw new IllegalArgumentException("Income mustn't be null.");
        }
        this.incomes.add(income);
        income.setUser(this);
    }
    public void removeIncome(Income income) {
        if (income == null) {
            return;
        }
        this.incomes.remove(income);
    }

    public List<Expense> getExpenses() {
        return expenses;
    }
    public void setExpenses(List<Expense> expenses) {
        this.expenses = expenses;
    }

    public void addExpense(Expense expense) {
        if (expense == null) {
            throw new IllegalArgumentException("Expense mustn't be null.");
        }
        this.expenses.add(expense);
        expense.setUser(this);
    }
    public void removeExpense(Expense expense) {
        if (expense == null) {
            return;
        }
        this.expenses.remove(expense);
    }


    public List<Budget> getBudgets() {
        return budgets;
    }
    public void setBudgets(List<Budget> budgets) {
        this.budgets = budgets;
    }

    public void addBudget(Budget budget) {
        if(budget == null) {
            throw new IllegalArgumentException("Income mustn't be null.");
        }
        this.budgets.add(budget);
        budget.setUser(this);
    }
    public void removeBudget(Budget budget) {
        if (budget == null) {
            return;
        }
        this.budgets.remove(budget);
    }

    @Override
    public boolean equals(Object o) {
        if (o == null || getClass() != o.getClass()) return false;
        User user = (User) o;
        return id != null && Objects.equals(id, user.id);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    @Override
    public String toString() {
        return "User{" +
                "id=" + id +
                ", firstName='" + firstName + '\'' +
                ", lastName='" + lastName + '\'' +
                ", email='" + email + '\'' +
                ", username='" + username + '\'' +
                '}';
    }
}
