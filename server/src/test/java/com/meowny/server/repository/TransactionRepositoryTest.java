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
import org.springframework.data.domain.Page;
import org.springframework.data.domain.PageRequest;
import org.springframework.data.domain.Pageable;
import org.springframework.data.domain.Sort;

import java.math.BigDecimal;
import java.time.LocalDate;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
public class TransactionRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private TransactionRepository transactionRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User savedUser;
    private Category savedCategory;

    @BeforeEach
    void setUp() {
        User user = new User();
        user.setFirstName("Vincent");
        user.setLastName("Meowny");
        user.setUsername("vincent_m");
        user.setEmail("vincent@meowny.com");
        user.setPassword("securePass");
        savedUser = entityManager.persistAndFlush(user);

        Category category = new Category();
        category.setName("Groceries");
        category.setType(TransactionType.EXPENSE);
        category.setUser(savedUser);
        savedCategory = entityManager.persistAndFlush(category);
    }

    @AfterEach
    void tearDown() {
        transactionRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should return true when a transaction exists with the target category ID")
    void shouldReturnTrueWhenTransactionExistsByCategoryId() {
        Transaction tx = createTransaction(savedUser, savedCategory, "Groceries Run", "85.20", LocalDate.now());
        entityManager.persistAndFlush(tx);

        boolean exists = transactionRepository.existsByCategoryId(savedCategory.getId());
        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should return true when a transaction is linked to the recurring template ID")
    void shouldReturnTrueWhenTransactionExistsBySourceTemplateId() {
        RecurringTransaction template = new RecurringTransaction();
        template.setUser(savedUser);
        template.setCategory(savedCategory);
        template.setType(TransactionType.EXPENSE);
        template.setName("Netflix Subscription");
        template.setAmount(new BigDecimal("15.99"));
        template.setNextDueDate(LocalDate.now().plusDays(5));
        template.setFrequency(Frequency.MONTHLY); // Assuming Frequency is an enum containing MONTHLY
        RecurringTransaction savedTemplate = entityManager.persistAndFlush(template);

        Transaction tx = createTransaction(savedUser, savedCategory, "Netflix Charge", "15.99", LocalDate.now());
        tx.setSourceTemplate(savedTemplate);
        entityManager.persistAndFlush(tx);

        boolean exists = transactionRepository.existsBySourceTemplate_Id(savedTemplate.getId());
        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should retrieve paginated transactions belonging to a user")
    void shouldFindTransactionsByUserIdWithPagination() {
        Transaction tx1 = createTransaction(savedUser, savedCategory, "Item A", "10.00", LocalDate.now().minusDays(2));
        Transaction tx2 = createTransaction(savedUser, savedCategory, "Item B", "20.00", LocalDate.now().minusDays(1));
        Transaction tx3 = createTransaction(savedUser, savedCategory, "Item C", "30.00", LocalDate.now());

        entityManager.persist(tx1);
        entityManager.persist(tx2);
        entityManager.persist(tx3);
        entityManager.flush();

        Pageable pageable = PageRequest.of(0, 2, Sort.by("amount").descending());

        Page<Transaction> page = transactionRepository.findByUserId(savedUser.getId(), pageable);

        assertThat(page.getTotalElements()).isEqualTo(3);
        assertThat(page.getContent()).hasSize(2);
        assertThat(page.getContent())
                .extracting("amount")
                .containsExactly(new BigDecimal("30.00"), new BigDecimal("20.00"));
    }

    @Test
    @DisplayName("Should retrieve paginated transactions filtered by user ID and category ID")
    void shouldFindTransactionsByUserIdAndCategoryIdWithPagination() {
        Category transportCategory = new Category();
        transportCategory.setName("Transport");
        transportCategory.setType(TransactionType.EXPENSE);
        transportCategory.setUser(savedUser);
        Category savedTransport = entityManager.persistAndFlush(transportCategory);

        Transaction tx1 = createTransaction(savedUser, savedCategory, "Supermarket", "50.00", LocalDate.now());
        Transaction tx2 = createTransaction(savedUser, savedTransport, "Train Ticket", "12.50", LocalDate.now());

        entityManager.persist(tx1);
        entityManager.persist(tx2);
        entityManager.flush();

        Pageable pageable = PageRequest.of(0, 10);

        Page<Transaction> transportPage = transactionRepository.findByUserIdAndCategoryId(
                savedUser.getId(),
                savedTransport.getId(),
                pageable
        );

        assertThat(transportPage.getTotalElements()).isEqualTo(1);
        assertThat(transportPage.getContent().get(0).getName()).isEqualTo("Train Ticket");
    }

    private Transaction createTransaction(User user, Category category, String name, String amount, LocalDate paymentDate) {
        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setType(category.getType());
        tx.setName(name);
        tx.setAmount(new BigDecimal(amount));
        tx.setPaymentDate(paymentDate);
        return tx;
    }
}