package com.meowny.server.service;

import com.meowny.server.entity.Category;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.dto.category.CategoryResponse;
import com.meowny.server.dto.category.CreateCategoryRequest;
import com.meowny.server.dto.category.UpdateCategoryRequest;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.*;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class CategoryServiceTest {

    @Mock
    private CategoryRepository categoryRepository;

    @Mock
    private UserRepository userRepository;

    @Mock
    private TransactionRepository transactionRepository;

    @Mock
    private BudgetRepository budgetRepository;

    @Mock
    private RecurringTransactionRepository recurringTransactionRepository;

    @InjectMocks
    private CategoryService categoryService;

    // ==========================================
    // getCategoriesByUserId BRANCHES
    // ==========================================

    @Test
    @DisplayName("getCategoriesByUserId: Should return list of category responses")
    void getCategoriesByUserId_ValidUser_ReturnsList() {
        Long userId = 1L;
        Category category = createMockCategory(10L, userId, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findByUserId(userId)).thenReturn(List.of(category));

        List<CategoryResponse> results = categoryService.getCategoriesByUserId(userId);

        assertThat(results).hasSize(1);
        assertThat(results.get(0).name()).isEqualTo("Food");
        verify(categoryRepository).findByUserId(userId);
    }

    // ==========================================
    // createCategory BRANCHES
    // ==========================================

    @Test
    @DisplayName("createCategory: Should create category successfully when valid and unique")
    void createCategory_ValidRequest_CreatesCategory() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, TransactionType.EXPENSE, "Salary");
        User user = new User();
        user.setId(1L);

        Category savedCategory = createMockCategory(10L, 1L, TransactionType.EXPENSE, "Salary");

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Salary")).thenReturn(Optional.empty());
        when(categoryRepository.save(any(Category.class))).thenReturn(savedCategory);

        CategoryResponse response = categoryService.createCategory(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(10L);
        verify(categoryRepository).save(any(Category.class));
    }

    @Test
    @DisplayName("createCategory: Should throw IllegalArgumentException when user does not exist")
    void createCategory_UserNotFound_ThrowsException() {
        CreateCategoryRequest request = new CreateCategoryRequest(99L, TransactionType.EXPENSE, "Salary");
        when(userRepository.findById(request.userId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + request.userId());

        verifyNoInteractions(categoryRepository);
    }

    @Test
    @DisplayName("createCategory: Should throw ResourceConflictException when category name already exists for user")
    void createCategory_NameExists_ThrowsResourceConflictException() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, TransactionType.EXPENSE, "Food");
        when(userRepository.findById(request.userId())).thenReturn(Optional.of(new User()));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Food")).thenReturn(Optional.of(new Category()));

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("A category with the name 'Food' already exists.");

        verify(categoryRepository, never()).save(any());
    }

    // ==========================================
    // updateCategoryName BRANCHES
    // ==========================================

    @Test
    @DisplayName("updateCategoryName: Should throw IllegalArgumentException when target category doesn't exist")
    void updateCategoryName_CategoryNotFound_ThrowsException() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("NewName");
        when(categoryRepository.findById(categoryId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.updateCategoryName(categoryId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + categoryId);

        verify(categoryRepository, never()).save(any());
    }

    @Test
    @DisplayName("updateCategoryName: Should save directly without checking unique name if name is unchanged")
    void updateCategoryName_NameUnchanged_SavesImmediately() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("Groceries");
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "groceries"); // case-insensitive match

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(existingCategory));
        when(categoryRepository.save(existingCategory)).thenReturn(existingCategory);

        CategoryResponse response = categoryService.updateCategoryName(categoryId, request);

        assertThat(response).isNotNull();
        verify(categoryRepository, never()).findByUserIdAndNameIgnoreCase(anyLong(), anyString());
        verify(categoryRepository).save(existingCategory);
    }

    @Test
    @DisplayName("updateCategoryName: Should update name when it is completely new and not taken")
    void updateCategoryName_NameChangedAndFree_UpdatesSuccessfully() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("Dining Out");
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Groceries");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(existingCategory));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Dining Out")).thenReturn(Optional.empty());
        when(categoryRepository.save(existingCategory)).thenReturn(existingCategory);

        CategoryResponse response = categoryService.updateCategoryName(categoryId, request);

        assertThat(response).isNotNull();
        verify(categoryRepository).findByUserIdAndNameIgnoreCase(1L, "Dining Out");
        assertThat(existingCategory.getName()).isEqualTo("Dining Out");
    }

    @Test
    @DisplayName("updateCategoryName: Should throw ResourceConflictException when trying to rename to an already existing name")
    void updateCategoryName_NameChangedAndTaken_ThrowsResourceConflictException() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("Rent");
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Groceries");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(existingCategory));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Rent")).thenReturn(Optional.of(new Category()));

        assertThatThrownBy(() -> categoryService.updateCategoryName(categoryId, request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Another category with the name 'Rent' already exists.");

        verify(categoryRepository, never()).save(any());
    }

    // ==========================================
    // deleteCategory BRANCHES
    // ==========================================

    @Test
    @DisplayName("deleteCategory: Should throw IllegalArgumentException when target category doesn't exist")
    void deleteCategory_CategoryNotFound_ThrowsException() {
        Long categoryId = 10L;
        when(categoryRepository.findById(categoryId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + categoryId);

        verify(categoryRepository, never()).delete(any());
    }

    @Test
    @DisplayName("deleteCategory: Should throw ResourceConflictException when linked to transactions")
    void deleteCategory_LinkedToTransactions_ThrowsResourceConflictException() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(transactionRepository.existsByCategoryId(categoryId)).thenReturn(true);

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete category because it is linked to existing transactions.");

        verifyNoInteractions(budgetRepository, recurringTransactionRepository);
        verify(categoryRepository, never()).delete(any());
    }

    @Test
    @DisplayName("deleteCategory: Should throw ResourceConflictException when linked to active budgets")
    void deleteCategory_LinkedToBudgets_ThrowsResourceConflictException() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(transactionRepository.existsByCategoryId(categoryId)).thenReturn(false);
        when(budgetRepository.existsByCategoryId(categoryId)).thenReturn(true);

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete category because it is assigned to an active budget.");

        verifyNoInteractions(recurringTransactionRepository);
        verify(categoryRepository, never()).delete(any());
    }

    @Test
    @DisplayName("deleteCategory: Should throw ResourceConflictException when linked to recurring transactions")
    void deleteCategory_LinkedToRecurringTransactions_ThrowsResourceConflictException() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(transactionRepository.existsByCategoryId(categoryId)).thenReturn(false);
        when(budgetRepository.existsByCategoryId(categoryId)).thenReturn(false);
        when(recurringTransactionRepository.existsByCategoryId(categoryId)).thenReturn(true);

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete category because it is linked to active recurring transactions.");

        verify(categoryRepository, never()).delete(any());
    }

    @Test
    @DisplayName("deleteCategory: Should complete deletion cleanly when no active references remain")
    void deleteCategory_NoReferences_DeletesSuccessfully() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(transactionRepository.existsByCategoryId(categoryId)).thenReturn(false);
        when(budgetRepository.existsByCategoryId(categoryId)).thenReturn(false);
        when(recurringTransactionRepository.existsByCategoryId(categoryId)).thenReturn(false);

        categoryService.deleteCategory(categoryId);

        verify(categoryRepository).delete(category);
    }

    // ==========================================
    // PRIVATE HELPER METHODS
    // ==========================================

    private Category createMockCategory(Long id, Long userId, TransactionType type, String name) {
        User user = new User();
        user.setId(userId);

        Category category = new Category();
        category.setId(id);
        category.setUser(user);
        category.setType(type);
        category.setName(name);
        return category;
    }
}