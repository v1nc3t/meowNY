package com.meowny.server.service;

import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.RecurringTransaction;
import com.meowny.commons.entity.Transaction;
import com.meowny.commons.entity.TransactionType;
import com.meowny.commons.entity.User;
import com.meowny.server.dto.transaction.CreateTransactionRequest;
import com.meowny.server.dto.transaction.TransactionResponse;
import com.meowny.server.dto.transaction.UpdateTransactionRequest;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.repository.UserRepository;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.PageImpl;
import org.springframework.data.domain.PageRequest;
import org.springframework.data.domain.Pageable;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class TransactionServiceTest {

    @Mock
    private TransactionRepository transactionRepository;

    @Mock
    private UserRepository userRepository;

    @Mock
    private CategoryRepository categoryRepository;

    @Mock
    private RecurringTransactionRepository recurringTransactionRepository;

    @InjectMocks
    private TransactionService transactionService;

    // ==========================================
    // getTransactionById BRANCHES
    // ==========================================

    @Test
    @DisplayName("getTransactionById: Should return response when transaction exists")
    void getTransactionById_Exists_ReturnsResponse() {
        Long txId = 100L;
        Transaction tx = createMockTransaction(txId, 1L, 2L, "Rent", TransactionType.EXPENSE, BigDecimal.valueOf(1000));

        when(transactionRepository.findById(txId)).thenReturn(Optional.of(tx));

        TransactionResponse response = transactionService.getTransactionById(txId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(txId);
        verify(transactionRepository).findById(txId);
    }

    @Test
    @DisplayName("getTransactionById: Should throw IllegalArgumentException when not found")
    void getTransactionById_NotFound_ThrowsException() {
        Long txId = 100L;
        when(transactionRepository.findById(txId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.getTransactionById(txId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction not found with ID: " + txId);
    }

    // ==========================================
    // getTransactionsByUserId BRANCHES
    // ==========================================

    @Test
    @DisplayName("getTransactionsByUserId: Should return mapped page of transactions")
    void getTransactionsByUserId_ValidUser_ReturnsPagedData() {
        Long userId = 1L;
        Pageable pageable = PageRequest.of(0, 10);
        Transaction tx = createMockTransaction(100L, userId, 2L, "Salary", TransactionType.INCOME, BigDecimal.valueOf(3000));
        Page<Transaction> page = new PageImpl<>(List.of(tx), pageable, 1);

        when(transactionRepository.findByUserId(userId, pageable)).thenReturn(page);

        Page<TransactionResponse> result = transactionService.getTransactionsByUserId(userId, pageable);

        assertThat(result.getContent()).hasSize(1);
        assertThat(result.getContent().get(0).id()).isEqualTo(100L);
        verify(transactionRepository).findByUserId(userId, pageable);
    }

    // ==========================================
    // createTransaction BRANCHES
    // ==========================================

    @Test
    @DisplayName("createTransaction: Should successfully save standard transaction without template link")
    void createTransaction_NoTemplate_CreatesSuccessfully() {
        CreateTransactionRequest request = new CreateTransactionRequest(
                1L, 2L, null, TransactionType.EXPENSE, "Lunch", BigDecimal.valueOf(15), LocalDate.now(), "Food truck"
        );

        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE); category.setName("Food");
        Transaction savedTx = createMockTransaction(100L, 1L, 2L, "Lunch", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(transactionRepository.save(any(Transaction.class))).thenReturn(savedTx);

        TransactionResponse response = transactionService.createTransaction(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(100L);
        verify(transactionRepository).save(any(Transaction.class));
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if user isn't found")
    void createTransaction_UserNotFound_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, null, TransactionType.EXPENSE, "Lunch", BigDecimal.valueOf(15), LocalDate.now(), "Food");
        when(userRepository.findById(request.userId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + request.userId());
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if category isn't found")
    void createTransaction_CategoryNotFound_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, null, TransactionType.EXPENSE, "Lunch", BigDecimal.valueOf(15), LocalDate.now(), "Food");
        when(userRepository.findById(request.userId())).thenReturn(Optional.of(new User()));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + request.categoryId());
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if category belongs to another user")
    void createTransaction_CategoryBelongsToAnother_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, null, TransactionType.EXPENSE, "Lunch", BigDecimal.valueOf(15), LocalDate.now(), "Food");
        User user = new User(); user.setId(1L);
        User otherUser = new User(); otherUser.setId(99L);
        Category category = new Category(); category.setId(2L); category.setUser(otherUser);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category must belong to the specified user.");
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if request transaction type differs from category config")
    void createTransaction_TypeMismatchWithCategory_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, null, TransactionType.INCOME, "Lunch", BigDecimal.valueOf(15), LocalDate.now(), "Food");
        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction type (INCOME) does not match Category type (EXPENSE).");
    }

    @Test
    @DisplayName("createTransaction: Should successfully map with template relation when cross-validations match")
    void createTransaction_ValidTemplate_LinksSuccessfully() {
        CreateTransactionRequest request = new CreateTransactionRequest(
                1L, 2L, 50L, TransactionType.EXPENSE, "Rent Bill", BigDecimal.valueOf(1000), LocalDate.now(), "Rent"
        );

        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE); category.setName("Housing");

        RecurringTransaction template = new RecurringTransaction();
        template.setId(50L); template.setUser(user); template.setType(TransactionType.EXPENSE); template.setCategory(category); template.setName("Rent Template");

        Transaction txToSave = createMockTransaction(100L, 1L, 2L, "Rent Bill", TransactionType.EXPENSE, BigDecimal.valueOf(1000));
        txToSave.setSourceTemplate(template);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.findById(50L)).thenReturn(Optional.of(template));
        when(transactionRepository.save(any(Transaction.class))).thenReturn(txToSave);

        TransactionResponse response = transactionService.createTransaction(request);

        assertThat(response).isNotNull();
        assertThat(response.recurringTransactionId()).isEqualTo(50L);
        assertThat(response.recurringTransactionName()).isEqualTo("Rent Template");
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if provided template ID does not exist")
    void createTransaction_TemplateIdNotFound_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, 50L, TransactionType.EXPENSE, "Rent Bill", BigDecimal.valueOf(1000), LocalDate.now(), "Rent");
        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.findById(50L)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Recurring template not found with ID: 50");
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if targeted template belongs to another account")
    void createTransaction_TemplateBelongsToOtherUser_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, 50L, TransactionType.EXPENSE, "Rent Bill", BigDecimal.valueOf(1000), LocalDate.now(), "Rent");
        User user = new User(); user.setId(1L);
        User otherUser = new User(); otherUser.setId(99L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE);

        RecurringTransaction template = new RecurringTransaction(); template.setId(50L); template.setUser(otherUser);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.findById(50L)).thenReturn(Optional.of(template));

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Recurring template must belong to the specified user.");
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if template configuration has type variant divergence")
    void createTransaction_TemplateTypeMismatch_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, 50L, TransactionType.EXPENSE, "Rent Bill", BigDecimal.valueOf(1000), LocalDate.now(), "Rent");
        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE);

        RecurringTransaction template = new RecurringTransaction(); template.setId(50L); template.setUser(user); template.setType(TransactionType.INCOME); // mismatch

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.findById(50L)).thenReturn(Optional.of(template));

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction type must match the recurring template type.");
    }

    @Test
    @DisplayName("createTransaction: Should throw exception if request category does not match template base category")
    void createTransaction_TemplateCategoryMismatch_ThrowsException() {
        CreateTransactionRequest request = new CreateTransactionRequest(1L, 2L, 50L, TransactionType.EXPENSE, "Rent Bill", BigDecimal.valueOf(1000), LocalDate.now(), "Rent");
        User user = new User(); user.setId(1L);
        Category requestCategory = new Category(); requestCategory.setId(2L); requestCategory.setUser(user); requestCategory.setType(TransactionType.EXPENSE);
        Category templateCategory = new Category(); templateCategory.setId(88L); // mismatch

        RecurringTransaction template = new RecurringTransaction();
        template.setId(50L); template.setUser(user); template.setType(TransactionType.EXPENSE); template.setCategory(templateCategory);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(requestCategory));
        when(recurringTransactionRepository.findById(50L)).thenReturn(Optional.of(template));

        assertThatThrownBy(() -> transactionService.createTransaction(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction category must match the recurring template category.");
    }

    // ==========================================
    // updateTransaction BRANCHES
    // ==========================================

    @Test
    @DisplayName("updateTransaction: Should successfully modify properties if categories are valid and owned")
    void updateTransaction_ValidInput_UpdatesSuccessfully() {
        Long txId = 100L;
        UpdateTransactionRequest request = new UpdateTransactionRequest(3L, "New Name", BigDecimal.valueOf(25), LocalDate.now(), "Desc");

        User user = new User(); user.setId(1L);
        Transaction existingTx = createMockTransaction(txId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        Category newCategory = new Category(); newCategory.setId(3L); newCategory.setUser(user); newCategory.setType(TransactionType.EXPENSE); newCategory.setName("New Cat");

        when(transactionRepository.findById(txId)).thenReturn(Optional.of(existingTx));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(newCategory));
        when(transactionRepository.save(existingTx)).thenReturn(existingTx);

        TransactionResponse response = transactionService.updateTransaction(txId, request);

        assertThat(response).isNotNull();
        assertThat(existingTx.getName()).isEqualTo("New Name");
        assertThat(existingTx.getCategory().getId()).isEqualTo(3L);
        verify(transactionRepository).save(existingTx);
    }

    @Test
    @DisplayName("updateTransaction: Should throw exception if transaction to alter doesn't exist")
    void updateTransaction_TxNotFound_ThrowsException() {
        Long txId = 100L;
        UpdateTransactionRequest request = new UpdateTransactionRequest(3L, "Name", BigDecimal.valueOf(25), LocalDate.now(), "Desc");
        when(transactionRepository.findById(txId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.updateTransaction(txId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Transaction not found with ID: " + txId);
    }

    @Test
    @DisplayName("updateTransaction: Should throw exception if destination category does not exist")
    void updateTransaction_CategoryNotFound_ThrowsException() {
        Long txId = 100L;
        UpdateTransactionRequest request = new UpdateTransactionRequest(3L, "Name", BigDecimal.valueOf(25), LocalDate.now(), "Desc");
        Transaction existingTx = createMockTransaction(txId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        when(transactionRepository.findById(txId)).thenReturn(Optional.of(existingTx));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> transactionService.updateTransaction(txId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + request.categoryId());
    }

    @Test
    @DisplayName("updateTransaction: Should throw exception if category swap references a profile workspace owned by another user")
    void updateTransaction_CategoryUserMismatch_ThrowsException() {
        Long txId = 100L;
        UpdateTransactionRequest request = new UpdateTransactionRequest(3L, "Name", BigDecimal.valueOf(25), LocalDate.now(), "Desc");

        Transaction existingTx = createMockTransaction(txId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        User otherUser = new User(); otherUser.setId(99L);
        Category targetCategory = new Category(); targetCategory.setId(3L); targetCategory.setUser(otherUser); // mismatch

        when(transactionRepository.findById(txId)).thenReturn(Optional.of(existingTx));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));

        assertThatThrownBy(() -> transactionService.updateTransaction(txId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category must belong to the transaction owner.");
    }

    @Test
    @DisplayName("updateTransaction: Should throw exception if mutating a transaction into an incompatible category variant type")
    void updateTransaction_CategoryTypeMismatch_ThrowsException() {
        Long txId = 100L;
        UpdateTransactionRequest request = new UpdateTransactionRequest(3L, "Name", BigDecimal.valueOf(25), LocalDate.now(), "Desc");

        User user = new User(); user.setId(1L);
        Transaction existingTx = createMockTransaction(txId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15)); // EXPENSE
        Category targetCategory = new Category(); targetCategory.setId(3L); targetCategory.setUser(user); targetCategory.setType(TransactionType.INCOME); targetCategory.setName("Investments"); // INCOME

        when(transactionRepository.findById(txId)).thenReturn(Optional.of(existingTx));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));

        assertThatThrownBy(() -> transactionService.updateTransaction(txId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Cannot assign to Category 'Investments' (INCOME) because this is an EXPENSE transaction.");
    }

    // ==========================================
    // PRIVATE HELPER METHODS
    // ==========================================

    private Transaction createMockTransaction(Long id, Long userId, Long categoryId, String categoryName, TransactionType type, BigDecimal amount) {
        User user = new User();
        user.setId(userId);

        Category category = new Category();
        category.setId(categoryId);
        category.setName(categoryName);
        category.setUser(user);
        category.setType(type);

        Transaction tx = new Transaction();
        tx.setId(id);
        tx.setUser(user);
        tx.setCategory(category);
        tx.setType(type);
        tx.setName(categoryName);
        tx.setAmount(amount);
        tx.setPaymentDate(LocalDate.now());
        return tx;
    }
}