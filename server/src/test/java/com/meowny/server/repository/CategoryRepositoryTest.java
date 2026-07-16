package com.meowny.server.repository;

import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.User;
import com.meowny.commons.entity.TransactionType; // Adjust package path if necessary
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.jdbc.AutoConfigureTestDatabase;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.boot.test.autoconfigure.orm.jpa.TestEntityManager;

import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
public class CategoryRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private CategoryRepository categoryRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User savedUser;

    @BeforeEach
    void setUp() {
        User user = new User();
        user.setFirstName("Jane");
        user.setLastName("Doe");
        user.setUsername("janedoe");
        user.setEmail("jane@example.com");
        user.setPassword("password123");

        savedUser = entityManager.persistAndFlush(user);
    }

    @AfterEach
    void tearDown() {
        categoryRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should find all categories belonging to a specific user")
    void shouldFindByUserId() {
        Category cat1 = createCategory("Food", TransactionType.EXPENSE, savedUser);
        Category cat2 = createCategory("Salary", TransactionType.INCOME, savedUser);
        entityManager.persist(cat1);
        entityManager.persist(cat2);
        entityManager.flush();

        List<Category> categories = categoryRepository.findByUserId(savedUser.getId());

        assertThat(categories).hasSize(2);
        assertThat(categories)
                .extracting("name")
                .containsExactlyInAnyOrder("Food", "Salary");
    }

    @Test
    @DisplayName("Should find categories filtered by user ID and transaction type")
    void shouldFindByUserIdAndType() {
        Category expense1 = createCategory("Rent", TransactionType.EXPENSE, savedUser);
        Category expense2 = createCategory("Groceries", TransactionType.EXPENSE, savedUser);
        Category income1 = createCategory("Investments", TransactionType.INCOME, savedUser);

        entityManager.persist(expense1);
        entityManager.persist(expense2);
        entityManager.persist(income1);
        entityManager.flush();

        List<Category> expenses = categoryRepository.findByUserIdAndType(savedUser.getId(), TransactionType.EXPENSE);

        assertThat(expenses).hasSize(2);
        assertThat(expenses)
                .extracting("name")
                .containsExactlyInAnyOrder("Rent", "Groceries");
    }

    @Test
    @DisplayName("Should find a category by name case-insensitively")
    void shouldFindByUserIdAndNameIgnoreCase() {
        Category cat = createCategory("Entertainment", TransactionType.EXPENSE, savedUser);
        entityManager.persistAndFlush(cat);

        Optional<Category> foundLower = categoryRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "entertainment");
        Optional<Category> foundUpper = categoryRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "ENTERTAINMENT");
        Optional<Category> foundMixed = categoryRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "EnTeRtAiNmEnT");

        assertThat(foundLower).isPresent();
        assertThat(foundUpper).isPresent();
        assertThat(foundMixed).isPresent();
        assertThat(foundLower.get().getName()).isEqualTo("Entertainment");
    }

    @Test
    @DisplayName("Should return empty optional when category name does not match")
    void shouldReturnEmptyWhenNameNotFound() {
        Optional<Category> found = categoryRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "NonExistentCategory");

        assertThat(found).isEmpty();
    }

    private Category createCategory(String name, TransactionType type, User user) {
        Category category = new Category();
        category.setName(name);
        category.setType(type);
        category.setUser(user);
        return category;
    }
}