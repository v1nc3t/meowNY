package com.meowny.server.service;

import com.meowny.server.dto.recurringtransaction.CreateRecurringTransactionRequest;
import com.meowny.server.dto.recurringtransaction.RecurringTransactionResponse;
import com.meowny.server.dto.recurringtransaction.UpdateRecurringTransactionRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.Frequency;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.security.CurrentUserService;
import com.meowny.server.support.TestCurrentUserSupport;
import org.junit.jupiter.api.BeforeEach;
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
    private CategoryRepository categoryRepository;

    @Mock
    private TransactionRepository transactionRepository;

    @Mock
    private CurrentUserService currentUserService;

    @InjectMocks
    private RecurringTransactionService recurringTransactionService;

    private User currentUser;

    @BeforeEach
    void setUp() {
        currentUser = new User();
        currentUser.setId(1L);
        TestCurrentUserSupport.stubCurrentUser(currentUserService, currentUser);
    }

    @Test
    @DisplayName("getCurrentUserTemplates: Should return list of mapped templates for valid user")
    void getCurrentUserTemplates_ValidUser_ReturnsMappedList() {
        RecurringTransaction template = createMockTemplate(100L, 1L, 2L, "Subscription", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        when(recurringTransactionRepository.findByUserId(1L)).thenReturn(List.of(template));

        List<RecurringTransactionResponse> results = recurringTransactionService.getCurrentUserTemplates();

        assertThat(results).hasSize(1);
        assertThat(results.get(0).id()).isEqualTo(100L);
        verify(recurringTransactionRepository).findByUserId(1L);
    }

    @Test
    @DisplayName("getTemplateById: Should return response when template exists")
    void getTemplateById_Exists_ReturnsResponse() {
        Long templateId = 100L;
        RecurringTransaction template = createMockTemplate(templateId, 1L, 2L, "Gym Membership", TransactionType.EXPENSE, BigDecimal.valueOf(50));
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(template));

        RecurringTransactionResponse response = recurringTransactionService.getTemplateById(templateId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(templateId);
        assertThat(response.type()).isEqualTo(TransactionType.EXPENSE);
    }

    @Test
    @DisplayName("getTemplateById: Should throw ResourceNotFoundException when template missing")
    void getTemplateById_NotFound_ThrowsException() {
        Long templateId = 100L;
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.getTemplateById(templateId))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("createTemplate: Should successfully save template when cross-validations pass")
    void createTemplate_ValidInput_CreatesSuccessfully() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(
                2L, "Netflix", BigDecimal.valueOf(15), LocalDate.now().plusDays(5), Frequency.MONTHLY
        );

        User user = new User();
        user.setId(1L);
        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Entertainment");
        RecurringTransaction savedTemplate = createMockTemplate(100L, 1L, 2L, "Netflix", TransactionType.EXPENSE, BigDecimal.valueOf(15));

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.save(any(RecurringTransaction.class))).thenReturn(savedTemplate);

        RecurringTransactionResponse response = recurringTransactionService.createTemplate(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(100L);
        assertThat(response.isActive()).isTrue();
        verify(recurringTransactionRepository).save(any(RecurringTransaction.class));
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if provided category does not exist")
    void createTemplate_CategoryNotFound_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(2L, "Netflix", BigDecimal.valueOf(15), LocalDate.now(), Frequency.MONTHLY);
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("createTemplate: Should throw exception if category matches an external user scope")
    void createTemplate_CategoryBelongsToAnother_ThrowsException() {
        CreateRecurringTransactionRequest request = new CreateRecurringTransactionRequest(2L, "Netflix", BigDecimal.valueOf(15), LocalDate.now(), Frequency.MONTHLY);
        User externalUser = new User();
        externalUser.setId(99L);
        Category category = new Category();
        category.setId(2L);
        category.setUser(externalUser);

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> recurringTransactionService.createTemplate(request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("updateTemplate: Should save configurations when modifying internal tracking parameters")
    void updateTemplate_ValidInput_UpdatesSuccessfully() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "New Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, false);

        User user = new User();
        user.setId(1L);
        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        Category targetCategory = new Category();
        targetCategory.setId(3L);
        targetCategory.setUser(user);
        targetCategory.setType(TransactionType.EXPENSE);
        targetCategory.setName("New Cat");

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
                .isInstanceOf(ResourceNotFoundException.class);
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
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("updateTemplate: Should throw exception if replacement category crosses tenant workspaces")
    void updateTemplate_CategoryOwnerMismatch_ThrowsException() {
        Long templateId = 100L;
        UpdateRecurringTransactionRequest request = new UpdateRecurringTransactionRequest(3L, "Name", BigDecimal.valueOf(20), LocalDate.now(), Frequency.WEEKLY, true);

        RecurringTransaction existingTemplate = createMockTemplate(templateId, 1L, 2L, "Old Name", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        User internalUser = new User();
        internalUser.setId(99L);
        Category targetCategory = new Category();
        targetCategory.setId(3L);
        targetCategory.setUser(internalUser);

        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(existingTemplate));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(targetCategory));

        assertThatThrownBy(() -> recurringTransactionService.updateTemplate(templateId, request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("deleteTemplate: Should complete successfully when object exists and has no historical dependencies")
    void deleteTemplate_Clean_DeletesSuccessfully() {
        Long templateId = 100L;
        RecurringTransaction template = createMockTemplate(templateId, 1L, 2L, "Subscription", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(template));
        when(transactionRepository.existsBySourceTemplate_Id(templateId)).thenReturn(false);

        recurringTransactionService.deleteTemplate(templateId);

        verify(recurringTransactionRepository).delete(template);
    }

    @Test
    @DisplayName("deleteTemplate: Should throw exception if entity identity does not exist")
    void deleteTemplate_NotFound_ThrowsException() {
        Long templateId = 100L;
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> recurringTransactionService.deleteTemplate(templateId))
                .isInstanceOf(ResourceNotFoundException.class);
        verify(recurringTransactionRepository, never()).delete(any());
    }

    @Test
    @DisplayName("deleteTemplate: Should throw ResourceConflictException if history tracking constraints prevent purging")
    void deleteTemplate_LinkedToTransactions_ThrowsConflictException() {
        Long templateId = 100L;
        RecurringTransaction template = createMockTemplate(templateId, 1L, 2L, "Subscription", TransactionType.EXPENSE, BigDecimal.valueOf(15));
        when(recurringTransactionRepository.findById(templateId)).thenReturn(Optional.of(template));
        when(transactionRepository.existsBySourceTemplate_Id(templateId)).thenReturn(true);

        assertThatThrownBy(() -> recurringTransactionService.deleteTemplate(templateId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete this template because it has generated past transaction records. Disable it instead.");
        verify(recurringTransactionRepository, never()).delete(any());
    }

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
        template.setName(name);
        template.setAmount(amount);
        template.setNextDueDate(LocalDate.now().plusDays(30));
        template.setFrequency(Frequency.MONTHLY);
        template.setActive(true);
        return template;
    }
}
