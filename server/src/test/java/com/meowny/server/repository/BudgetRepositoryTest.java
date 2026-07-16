package com.meowny.server.repository;

import com.meowny.commons.entity.Budget;
import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.TransactionType;
import com.meowny.commons.entity.User;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.jdbc.AutoConfigureTestDatabase;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.boot.test.autoconfigure.orm.jpa.TestEntityManager;

import java.math.BigDecimal;
import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
class BudgetRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private BudgetRepository budgetRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User savedUser;
    private Category savedCategory;

    @BeforeEach
    void setUp() {
        User user = new User();
        user.setFirstName("John");
        user.setLastName("Doe");
        user.setUsername("johndoe");
        user.setEmail("john@example.com");
        user.setPassword("securePassword");
        savedUser = entityManager.persistAndFlush(user);

        Category category = new Category();
        category.setName("Utilities");
        category.setType(TransactionType.EXPENSE);
        category.setUser(savedUser);
        savedCategory = entityManager.persistAndFlush(category);
    }

    @AfterEach
    void tearDown() {
        budgetRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should find budget by user ID, category ID, month, and year")
    void shouldFindByUserIdAndCategoryIdAndMonthAndYear() {
        Budget budget = createBudget(savedUser, savedCategory, 5, 2026, "150.00");
        entityManager.persistAndFlush(budget);

        Optional<Budget> found = budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(
                savedUser.getId(),
                savedCategory.getId(),
                5,
                2026
        );

        assertThat(found).isPresent();
        assertThat(found.get().getLimitAmount()).isEqualByComparingTo("150.00");
    }

    @Test
    @DisplayName("Should return empty optional when no budget matches search criteria")
    void shouldReturnEmptyWhenBudgetNotFound() {
        Optional<Budget> found = budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(
                savedUser.getId(),
                savedCategory.getId(),
                12,
                2026
        );

        assertThat(found).isEmpty();
    }

    @Test
    @DisplayName("Should find all budgets for a user in a specific year and month")
    void shouldFindByUserIdAndYearAndMonth() {
        Category otherCategory = new Category();
        otherCategory.setName("Groceries");
        otherCategory.setType(TransactionType.EXPENSE);
        otherCategory.setUser(savedUser);
        Category savedOtherCategory = entityManager.persistAndFlush(otherCategory);

        Budget budget1 = createBudget(savedUser, savedCategory, 7, 2026, "200.00");
        Budget budget2 = createBudget(savedUser, savedOtherCategory, 7, 2026, "350.00");
        Budget budgetOtherMonth = createBudget(savedUser, savedCategory, 8, 2026, "100.00");

        entityManager.persist(budget1);
        entityManager.persist(budget2);
        entityManager.persist(budgetOtherMonth);
        entityManager.flush();

        List<Budget> budgets = budgetRepository.findByUserIdAndYearAndMonth(savedUser.getId(), 2026, 7);

        assertThat(budgets).hasSize(2);

        assertThat(budgets)
                .extracting("limitAmount")
                .containsExactlyInAnyOrder(new BigDecimal("200.00"), new BigDecimal("350.00"));
    }

    @Test
    @DisplayName("Should delete budget using user ID and category ID")
    void shouldDeleteByUserIdAndCategoryId() {
        Budget budget = createBudget(savedUser, savedCategory, 6, 2026, "500.00");
        entityManager.persistAndFlush(budget);

        budgetRepository.deleteByUserIdAndCategoryId(savedUser.getId(), savedCategory.getId());
        entityManager.flush();
        entityManager.clear();

        Optional<Budget> remaining = budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(
                savedUser.getId(),
                savedCategory.getId(),
                6,
                2026
        );
        assertThat(remaining).isEmpty();
    }

    @Test
    @DisplayName("Should return true when a budget with the category ID exists")
    void shouldReturnTrueWhenCategoryExistsInBudgets() {
        Budget budget = createBudget(savedUser, savedCategory, 1, 2026, "50.00");
        entityManager.persistAndFlush(budget);

        boolean exists = budgetRepository.existsByCategoryId(savedCategory.getId());

        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should return false when no budget uses the category ID")
    void shouldReturnFalseWhenCategoryDoesNotExistInBudgets() {
        boolean exists = budgetRepository.existsByCategoryId(savedCategory.getId());

        assertThat(exists).isFalse();
    }

    private Budget createBudget(User user, Category category, Integer month, Integer year, String limitAmount) {
        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setMonth(month);
        budget.setYear(year);
        budget.setLimitAmount(new BigDecimal(limitAmount));
        return budget;
    }
}
