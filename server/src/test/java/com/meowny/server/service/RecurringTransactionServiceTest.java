package com.meowny.server.service;

import com.meowny.server.entity.Category;
import com.meowny.server.entity.Frequency;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.dto.recurringtransaction.CreateRecurringTransactionRequest;
import com.meowny.server.dto.recurringtransaction.RecurringTransactionResponse;
import com.meowny.server.dto.recurringtransaction.UpdateRecurringTransactionRequest;
import com.meowny.server.exception.ResourceConflictException;
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

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class RecurringTransactionServiceTest {

    @Mock
    private RecurringTransactionRepository recurringTransactionRepository;

    @Mock
    private UserRepository userRepository;

    @Mock
    private CategoryRepository categoryRepository;

    @Mock
    private TransactionRepository transactionRepository;

    @InjectMocks
    private RecurringTransactionService recurringTransactionService;

    // ==========================================
    // getTemplatesByUserId BRANCHES
    // ==========================================

    @Test
    @DisplayName("getTemplatesByUserId: Should return list of mapped templates for valid user")
    void getTemplatesByUserId_ValidUser_ReturnsMappedList() {
        Long userId = 1L;
        RecurringTransaction template = createMockTemplate(100L, userId, 2L, "Subscription", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        when(recurringTransactionRepository.findByUserId(userId)).thenReturn(List.of(template));

        List<RecurringTransactionResponse> results = recurringTransactionService.getTemplatesByUserId(userId);

        assertThat(results).hasSize(1);
        assertThat(results.get(0).id()).isEqualTo(100L);
        verify(recurringTransactionRepository).findByUserId(userId);
    }

    // ==========================================
    // getTemplateById BRANCHES
    // ==========================================

    @Test
    @DisplayName("getTemplateById: Should return response when template exists")
    void getTemplateById_Exists_ReturnsResponse() {
        Long templateId = 100L;
        RecurringTransaction template = createMockTemplate(templateId, 1L, 2L, "Gym Membership", TransactionType.EXPENSE, BigDecimal.valueOf(50));
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(template));

        RecurringTransactionResponse response = recurringTransactionService.getTemplateById(templateId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(templateId);
    }

    @Test
    @DisplayName("getTemplateById: Should throw IllegalArgumentException when template missing")
    void getTemplateById_NotFound_ThrowsException() {
        Long templateId = 100L;
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.getTemplateById(templateId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Recurring template not found with ID: " + templateId);
    }

    // ==========================================
    // createTemplate BRANCHES
    // ==========================================

    @Test
    @DisplayName("createTemplate: Should successfully save template when cross-validations pass")
    void createTemplate_ValidInput_CreatesSuccessfully() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(
                1L, 2L, TransactionType.EXPENSE, "Netflix", BigDecimal.valueOf(15), LocalDate.now().plusDays(5), Frequency.MONTHLY
        );

        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE); category.setName("Entertainment");
        RecurringTransaction savedTemplate = createMockTemplate(100L, 1L, 2L, "Netflix", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.save(any(RecurringTransaction.class))).thenReturn(savedTemplate);

        RecurringTransactionResponse response = recurringTransactionService.createTemplate(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(100L);
        assertThat(response.isActive()).isTrue();
        verify(recurringTransactionRepository).save(any(RecurringTransaction.class));
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if target user context is absent")
    void createTemplate_UserNotFound_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(1L, 2L, TransactionType.EXPENSE, "Netflix", BigDecimal.valueOf(15), LocalDate.now(), Frequency.MONTHLY);
        when(userRepository.findById(request.userId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + request.userId());
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if provided category does not exist")
    void createTemplate_CategoryNotFound_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(1L, 2L, TransactionType.EXPENSE, "Netflix", BigDecimal.valueOf(15), LocalDate.now(), Frequency.MONTHLY);
        when(userRepository.findById(request.userId())).thenReturn(Optional.of(new User()));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + request.categoryId());
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if category matches an external user scope")
    void createTemplate_CategoryBelongsToAnother_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(1L, 2L, TransactionType.EXPENSE, "Netflix", BigDecimal.valueOf(15), LocalDate.now(), Frequency.MONTHLY);
        User user = new User(); user.setId(1L);
        User externalUser = new User(); externalUser.setId(99L);
        Category category = new Category(); category.setId(2L); category.setUser(externalUser);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category must belong to the specified user.");
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if template operation variant opposes category type")
    void createTemplate_TypeMismatchWithCategory_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(1L, 2L, TransactionType.INCOME, "Salary Template", BigDecimal.valueOf(3000), LocalDate.now(), Frequency.MONTHLY);
        User user = new User(); user.setId(1L);
        Category category = new Category(); category.setId(2L); category.setUser(user); category.setType(TransactionType.EXPENSE);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Template transaction type (INCOME) must match Category type (EXPENSE).");
    }

    // ==========================================
    // updateTemplate BRANCHES
    // ==========================================

    @Test
    @DisplayName("updateTemplate: Should save configurations when modifying internal tracking parameters")
    void updateTemplate_ValidInput_UpdatesSuccessfully() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "New Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, false);

        User user = new User(); user.setId(1L);
        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        Category targetCategory = new Category(); targetCategory.setId(3L); targetCategory.setUser(user); targetCategory.setType(TransactionType.EXPENSE); targetCategory.setName("New Cat");

        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(existingTemplate));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));
        when(recurringTransactionRepository.save(existingTemplate)).thenReturn(existingTemplate);

        RecurringTransactionResponse response = recurringTransactionService.updateTemplate(templateId, request);

        assertThat(response).isNotNull();
        assertThat(existingTemplate.getName()).isEqualTo("New Name");
        assertThat(existingTemplate.isActive()).isFalse();
        verify(recurringTransactionRepository).save(existingTemplate);
    }

    @Test
    @DisplayName("updateTemplate: Should throw exception if targeting missing template payload")
    void updateTemplate_TemplateNotFound_ThrowsException() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, true);
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.updateTemplate(templateId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Recurring template not found with ID: " + templateId);
    }

    @Test
    @DisplayName("updateTemplate: Should throw exception if replacement category identity is absent")
    void updateTemplate_CategoryNotFound_ThrowsException() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, true);
        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(existingTemplate));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.updateTemplate(templateId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + request.categoryId());
    }

    @Test
    @DisplayName("updateTemplate: Should throw exception if replacement category crosses tenant workspaces")
    void updateTemplate_CategoryOwnerMismatch_ThrowsException() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, true);

        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        User internalUser = new User(); internalUser.setId(99L);
        Category targetCategory = new Category(); targetCategory.setId(3L); targetCategory.setUser(internalUser); // mismatch

        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(existingTemplate));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));

        assertThatThrownBy(() -> recurringTransactionService.updateTemplate(templateId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category must belong to the template owner.");
    }

    @Test
    @DisplayName("updateTemplate: Should throw exception if category swap mutates transactional nature variant")
    void updateTemplate_CategoryTypeMismatch_ThrowsException() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, true);

        User user = new User(); user.setId(1L);
        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        Category targetCategory = new Category(); targetCategory.setId(3L); targetCategory.setUser(user); targetCategory.setType(TransactionType.INCOME); targetCategory.setName("Dividends"); // mismatch

        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(existingTemplate));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));

        assertThatThrownBy(() -> recurringTransactionService.updateTemplate(templateId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Cannot assign to Category 'Dividends' (INCOME) because this is an EXPENSE recurring template.");
    }

    // ==========================================
    // deleteTemplate BRANCHES
    // ==========================================

    @Test
    @DisplayName("deleteTemplate: Should complete successfully when object exists and has no historical dependencies")
    void deleteTemplate_Clean_DeletesSuccessfully() {
        Long templateId = 100L;
        when(recurringTransactionRepository.existsById(templateId)).thenReturn(true);
        when(transactionRepository.existsBySourceTemplate_Id(templateId)).thenReturn(false);

        recurringTransactionService.deleteTemplate(templateId);

        verify(recurringTransactionRepository).deleteById(templateId);
    }

    @Test
    @DisplayName("deleteTemplate: Should throw exception if entity identity does not exist")
    void deleteTemplate_NotFound_ThrowsException() {
        // Given
        Long templateId = 100L;
        when(recurringTransactionRepository.existsById(templateId)).thenReturn(false);

        // When & Then
        assertThatThrownBy(() -> recurringTransactionService.deleteTemplate(templateId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Recurring template not found with ID: " + templateId);
        verify(recurringTransactionRepository, never()).deleteById(anyLong());
    }

    @Test
    @DisplayName("deleteTemplate: Should throw ResourceConflictException if history tracking constraints prevent purging")
    void deleteTemplate_LinkedToTransactions_ThrowsConflictException() {
        Long templateId = 100L;
        when(recurringTransactionRepository.existsById(templateId)).thenReturn(true);
        when(transactionRepository.existsBySourceTemplate_Id(templateId)).thenReturn(true);

        assertThatThrownBy(() -> recurringTransactionService.deleteTemplate(templateId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete this template because it has generated past transaction records. Disable it instead.");
        verify(recurringTransactionRepository, never()).deleteById(anyLong());
    }

    // ==========================================
    // PRIVATE HELPER METHODS
    // ==========================================

    private RecurringTransaction createMockTemplate(Long id, Long userId, Long categoryId, String name, TransactionType type, BigDecimal amount) {
        User user = new User();
        user.setId(userId);

        Category category = new Category();
        category.setId(categoryId);
        category.setName(name);
        category.setUser(user);
        category.setType(type);

        RecurringTransaction template = new RecurringTransaction();
        template.setId(id);
        template.setUser(user);
        template.setCategory(category);
        template.setType(type);
        template.setName(name);
        template.setAmount(amount);
        template.setNextDueDate(LocalDate.now().plusDays(30));
        template.setFrequency(Frequency.MONTHLY);
        template.setActive(true);
        return template;
    }
}