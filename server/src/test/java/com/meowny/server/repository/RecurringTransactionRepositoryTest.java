package com.meowny.server.repository;

import com.meowny.server.entity.*;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.jdbc.AutoConfigureTestDatabase;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.boot.test.autoconfigure.orm.jpa.TestEntityManager;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
public class RecurringTransactionRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private RecurringTransactionRepository recurringTransactionRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User savedUser;
    private Category savedCategory;

    @BeforeEach
    void setUp() {
        User user = new User();
        user.setFirstName("Alice");
        user.setLastName("Smith");
        user.setUsername("alicesmith");
        user.setEmail("alice@example.com");
        user.setPassword("securePass1");
        savedUser = entityManager.persistAndFlush(user);

        Category category = new Category();
        category.setName("Subscriptions");
        category.setType(TransactionType.EXPENSE);
        category.setUser(savedUser);
        savedCategory = entityManager.persistAndFlush(category);
    }

    @AfterEach
    void tearDown() {
        recurringTransactionRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should find only active recurring transactions for a user")
    void shouldFindByUserIdAndIsActiveTrue() {
        RecurringTransaction activeTemplate = createTemplate("Gym Membership", "45.00", LocalDate.now(), true);
        RecurringTransaction inactiveTemplate = createTemplate("Old Service", "10.00", LocalDate.now(), false);

        entityManager.persist(activeTemplate);
        entityManager.persist(inactiveTemplate);
        entityManager.flush();

        List<RecurringTransaction> results = recurringTransactionRepository.findByUserIdAndIsActiveTrue(savedUser.getId());

        assertThat(results).hasSize(1);
        assertThat(results.get(0).getName()).isEqualTo("Gym Membership");
    }

    @Test
    @DisplayName("Should find all recurring transactions (active and inactive) for a user")
    void shouldFindByUserId() {
        RecurringTransaction activeTemplate = createTemplate("Gym Membership", "45.00", LocalDate.now(), true);
        RecurringTransaction inactiveTemplate = createTemplate("Old Service", "10.00", LocalDate.now(), false);

        entityManager.persist(activeTemplate);
        entityManager.persist(inactiveTemplate);
        entityManager.flush();

        List<RecurringTransaction> results = recurringTransactionRepository.findByUserId(savedUser.getId());

        assertThat(results).hasSize(2);
        assertThat(results).extracting("name")
                .containsExactlyInAnyOrder("Gym Membership", "Old Service");
    }

    @Test
    @DisplayName("Should lock and fetch active templates that are overdue or due today")
    void shouldFindOverdueTemplatesForUpdate() {
        LocalDate today = LocalDate.now();


        RecurringTransaction overdue = createTemplate("Overdue Bill", "100.00", today.minusDays(1), true);

        RecurringTransaction dueToday = createTemplate("Due Today Bill", "50.00", today, true);

        RecurringTransaction future = createTemplate("Future Bill", "75.00", today.plusDays(1), true);

        RecurringTransaction inactiveOverdue = createTemplate("Inactive Overdue", "20.00", today.minusDays(2), false);

        entityManager.persist(overdue);
        entityManager.persist(dueToday);
        entityManager.persist(future);
        entityManager.persist(inactiveOverdue);
        entityManager.flush();

        List<RecurringTransaction> results = recurringTransactionRepository.findOverdueTemplatesForUpdate(today);

        assertThat(results).hasSize(2);
        assertThat(results).extracting("name")
                .containsExactlyInAnyOrder("Overdue Bill", "Due Today Bill");
    }

    @Test
    @DisplayName("Should return true if a template exists under the target category ID")
    void shouldReturnTrueWhenTemplateExistsByCategoryId() {
        RecurringTransaction template = createTemplate("Internet Service", "60.00", LocalDate.now(), true);
        entityManager.persistAndFlush(template);

        boolean exists = recurringTransactionRepository.existsByCategoryId(savedCategory.getId());
        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should return false when no templates match the category ID")
    void shouldReturnFalseWhenNoTemplateExistsByCategoryId() {
        boolean exists = recurringTransactionRepository.existsByCategoryId(savedCategory.getId());
        assertThat(exists).isFalse();
    }

    private RecurringTransaction createTemplate(String name, String amount, LocalDate nextDueDate, boolean isActive) {
        RecurringTransaction rt = new RecurringTransaction();
        rt.setUser(savedUser);
        rt.setCategory(savedCategory);
        rt.setType(savedCategory.getType());
        rt.setName(name);
        rt.setAmount(new BigDecimal(amount));
        rt.setNextDueDate(nextDueDate);
        rt.setFrequency(Frequency.MONTHLY);
        rt.setActive(isActive);
        return rt;
    }
}