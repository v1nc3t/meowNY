package com.meowny.server.service;

import com.meowny.server.dto.category.CategoryResponse;
import com.meowny.server.dto.category.CreateCategoryRequest;
import com.meowny.server.dto.category.UpdateCategoryRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.CategoryGroup;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryGroupRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.UserRepository;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.LocalDateTime;
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
    private CategoryGroupRepository categoryGroupRepository;

    @Mock
    private UserRepository userRepository;

    @Mock
    private RecurringTransactionRepository recurringTransactionRepository;

    @Mock
    private BudgetRepository budgetRepository;

    @InjectMocks
    private CategoryService categoryService;

    @Test
    @DisplayName("getCategoriesByUserId: Should return list of category responses")
    void getCategoriesByUserId_ValidUser_ReturnsList() {
        Long userId = 1L;
        Category category = createMockCategory(10L, userId, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findByUserIdAndDeletedAtIsNull(userId)).thenReturn(List.of(category));

        List<CategoryResponse> results = categoryService.getCategoriesByUserId(userId);

        assertThat(results).hasSize(1);
        assertThat(results.get(0).name()).isEqualTo("Food");
        verify(categoryRepository).findByUserIdAndDeletedAtIsNull(userId);
    }

    @Test
    @DisplayName("createCategory: Should create category successfully when valid and unique")
    void createCategory_ValidRequest_CreatesCategory() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, null, TransactionType.EXPENSE, "Salary");
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
    @DisplayName("createCategory: Should assign category group when a valid group ID is provided")
    void createCategory_ValidGroup_AssignsGroup() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, 5L, TransactionType.EXPENSE, "Rent");
        User user = new User();
        user.setId(1L);

        CategoryGroup group = new CategoryGroup();
        group.setId(5L);
        group.setName("Housing");
        group.setUser(user);

        Category savedCategory = createMockCategory(10L, 1L, TransactionType.EXPENSE, "Rent");
        savedCategory.setCategoryGroup(group);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Rent")).thenReturn(Optional.empty());
        when(categoryGroupRepository.findById(5L)).thenReturn(Optional.of(group));
        when(categoryRepository.save(any(Category.class))).thenReturn(savedCategory);

        CategoryResponse response = categoryService.createCategory(request);

        assertThat(response.categoryGroupId()).isEqualTo(5L);
        assertThat(response.categoryGroupName()).isEqualTo("Housing");
    }

    @Test
    @DisplayName("createCategory: Should throw IllegalArgumentException when user does not exist")
    void createCategory_UserNotFound_ThrowsException() {
        CreateCategoryRequest request = new CreateCategoryRequest(99L, null, TransactionType.EXPENSE, "Salary");
        when(userRepository.findById(request.userId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + request.userId());

        verifyNoInteractions(categoryRepository);
    }

    @Test
    @DisplayName("createCategory: Should throw ResourceConflictException when category name already exists for user")
    void createCategory_NameExists_ThrowsResourceConflictException() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, null, TransactionType.EXPENSE, "Food");
        when(userRepository.findById(request.userId())).thenReturn(Optional.of(new User()));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Food")).thenReturn(Optional.of(new Category()));

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("A category with the name 'Food' already exists.");

        verify(categoryRepository, never()).save(any());
    }

    @Test
    @DisplayName("createCategory: Should throw IllegalArgumentException when category group does not exist")
    void createCategory_GroupNotFound_ThrowsException() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, 99L, TransactionType.EXPENSE, "Food");
        User user = new User();
        user.setId(1L);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Food")).thenReturn(Optional.empty());
        when(categoryGroupRepository.findById(99L)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category group not found with ID: 99");
    }

    @Test
    @DisplayName("createCategory: Should throw IllegalArgumentException when category group belongs to another user")
    void createCategory_GroupBelongsToAnotherUser_ThrowsException() {
        CreateCategoryRequest request = new CreateCategoryRequest(1L, 5L, TransactionType.EXPENSE, "Food");
        User user = new User();
        user.setId(1L);
        User otherUser = new User();
        otherUser.setId(2L);

        CategoryGroup group = new CategoryGroup();
        group.setId(5L);
        group.setUser(otherUser);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Food")).thenReturn(Optional.empty());
        when(categoryGroupRepository.findById(5L)).thenReturn(Optional.of(group));

        assertThatThrownBy(() -> categoryService.createCategory(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category group must belong to the specified user.");
    }

    @Test
    @DisplayName("updateCategoryName: Should throw IllegalArgumentException when target category doesn't exist")
    void updateCategoryName_CategoryNotFound_ThrowsException() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("NewName", null);
        when(categoryRepository.findById(categoryId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.updateCategoryName(categoryId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + categoryId);

        verify(categoryRepository, never()).save(any());
    }

    @Test
    @DisplayName("updateCategoryName: Should throw IllegalArgumentException when category is soft-deleted")
    void updateCategoryName_CategorySoftDeleted_ThrowsException() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("NewName", null);
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Groceries");
        existingCategory.setDeletedAt(LocalDateTime.now());

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(existingCategory));

        assertThatThrownBy(() -> categoryService.updateCategoryName(categoryId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + categoryId);
    }

    @Test
    @DisplayName("updateCategoryName: Should save without checking unique name if name is unchanged")
    void updateCategoryName_NameUnchanged_SavesImmediately() {
        Long categoryId = 10L;
        UpdateCategoryRequest request = new UpdateCategoryRequest("Groceries", null);
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "groceries");

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
        UpdateCategoryRequest request = new UpdateCategoryRequest("Dining Out", null);
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
        UpdateCategoryRequest request = new UpdateCategoryRequest("Rent", null);
        Category existingCategory = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Groceries");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(existingCategory));
        when(categoryRepository.findByUserIdAndNameIgnoreCase(1L, "Rent")).thenReturn(Optional.of(new Category()));

        assertThatThrownBy(() -> categoryService.updateCategoryName(categoryId, request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Another category with the name 'Rent' already exists.");

        verify(categoryRepository, never()).save(any());
    }

    @Test
    @DisplayName("deleteCategory: Should throw IllegalArgumentException when target category doesn't exist")
    void deleteCategory_CategoryNotFound_ThrowsException() {
        Long categoryId = 10L;
        when(categoryRepository.findById(categoryId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + categoryId);

        verify(categoryRepository, never()).save(any());
    }

    @Test
    @DisplayName("deleteCategory: Should throw ResourceConflictException when linked to recurring transactions")
    void deleteCategory_LinkedToRecurringTransactions_ThrowsResourceConflictException() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.existsByCategoryId(categoryId)).thenReturn(true);

        assertThatThrownBy(() -> categoryService.deleteCategory(categoryId))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Cannot delete category because it is linked to active recurring transactions.");

        verify(categoryRepository, never()).save(any());
        verifyNoInteractions(budgetRepository);
    }

    @Test
    @DisplayName("deleteCategory: Should soft-delete when no recurring transactions remain")
    void deleteCategory_NoReferences_SoftDeletesSuccessfully() {
        Long categoryId = 10L;
        Category category = createMockCategory(categoryId, 1L, TransactionType.EXPENSE, "Food");

        when(categoryRepository.findById(categoryId)).thenReturn(Optional.of(category));
        when(recurringTransactionRepository.existsByCategoryId(categoryId)).thenReturn(false);

        categoryService.deleteCategory(categoryId);

        assertThat(category.getDeletedAt()).isNotNull();
        assertThat(category.isDeleted()).isTrue();
        verify(budgetRepository).deleteByUserIdAndCategoryId(1L, categoryId);
        verify(categoryRepository).save(category);
        verify(categoryRepository, never()).delete(any());
    }

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
