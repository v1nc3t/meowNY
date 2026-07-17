package com.meowny.server.service;

import com.meowny.commons.entity.Budget;
import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.TransactionType;
import com.meowny.commons.entity.User;
import com.meowny.server.dto.budget.BudgetResponse;
import com.meowny.server.dto.budget.CreateBudgetRequest;
import com.meowny.server.dto.budget.UpdateBudgetRequest;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.UserRepository;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.math.BigDecimal;
import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class BudgetServiceTest {

    @Mock
    private BudgetRepository budgetRepository;

    @Mock
    private UserRepository userRepository;

    @Mock
    private CategoryRepository categoryRepository;

    @InjectMocks
    private BudgetService budgetService;

    // ==========================================
    // getBudgetById BRANCHES
    // ==========================================

    @Test
    @DisplayName("getBudgetById: Should return BudgetResponse when budget exists")
    void getBudgetById_BudgetExists_ReturnsResponse() {
        Long budgetId = 100L;
        Budget budget = createMockBudget(budgetId, 1L, 2L, "Groceries", BigDecimal.valueOf(500), 5, 2026);

        when(budgetRepository.findById(budgetId)).thenReturn(Optional.of(budget));

        BudgetResponse response = budgetService.getBudgetById(budgetId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(budgetId);
        assertThat(response.categoryName()).isEqualTo("Groceries");
        verify(budgetRepository).findById(budgetId);
    }

    @Test
    @DisplayName("getBudgetById: Should throw IllegalArgumentException when budget not found")
    void getBudgetById_BudgetNotFound_ThrowsException() {
        Long budgetId = 100L;
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.getBudgetById(budgetId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Budget not found with ID: " + budgetId);
    }

    // ==========================================
    // getBudgetsByPeriod BRANCHES
    // ==========================================

    @Test
    @DisplayName("getBudgetsByPeriod: Should return list of budget responses")
    void getBudgetsByPeriod_ValidInput_ReturnsList() {
        Long userId = 1L;
        Integer year = 2026;
        Integer month = 5;
        Budget budget = createMockBudget(100L, userId, 2L, "Rent", BigDecimal.valueOf(1200), month, year);

        when(budgetRepository.findByUserIdAndYearAndMonth(userId, year, month))
                .thenReturn(List.of(budget));

        List<BudgetResponse> results = budgetService.getBudgetsByPeriod(userId, year, month);

        assertThat(results).hasSize(1);
        assertThat(results.get(0).limitAmount()).isEqualTo(BigDecimal.valueOf(1200));
        verify(budgetRepository).findByUserIdAndYearAndMonth(userId, year, month);
    }

    // ==========================================
    // createBudget BRANCHES
    // ==========================================

    @Test
    @DisplayName("createBudget: Should create budget successfully under valid conditions")
    void createBudget_ValidRequest_CreatesBudget() {
        CreateBudgetRequest request = new CreateBudgetRequest(1L, 2L, BigDecimal.valueOf(300), 5, 2026);

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Utilities");

        Budget savedBudget = createMockBudget(100L, 1L, 2L, "Utilities", BigDecimal.valueOf(300), 5, 2026);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(1L, 2L, 5, 2026))
                .thenReturn(Optional.empty());
        when(budgetRepository.save(any(Budget.class))).thenReturn(savedBudget);

        BudgetResponse response = budgetService.createBudget(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(100L);
        verify(budgetRepository).save(any(Budget.class));
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when user does not exist")
    void createBudget_UserNotFound_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(99L, 2L, BigDecimal.valueOf(300), 5, 2026);
        when(userRepository.findById(request.userId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + request.userId());

        verifyNoInteractions(categoryRepository, budgetRepository);
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when category does not exist")
    void createBudget_CategoryNotFound_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(1L, 99L, BigDecimal.valueOf(300), 5, 2026);
        when(userRepository.findById(request.userId())).thenReturn(Optional.of(new User()));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category not found with ID: " + request.categoryId());

        verifyNoInteractions(budgetRepository);
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when category belongs to a different user")
    void createBudget_CategoryBelongsToAnotherUser_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(1L, 2L, BigDecimal.valueOf(300), 5, 2026);

        User actualUser = new User();
        actualUser.setId(1L);

        User wrongUser = new User();
        wrongUser.setId(55L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(wrongUser);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(actualUser));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category must belong to the specified user.");

        verifyNoInteractions(budgetRepository);
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when category type is INCOME")
    void createBudget_CategoryNotExpense_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(1L, 2L, BigDecimal.valueOf(300), 5, 2026);

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.INCOME);

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Budgets can only be created for EXPENSE categories.");

        verifyNoInteractions(budgetRepository);
    }

    @Test
    @DisplayName("createBudget: Should throw ResourceConflictException when a budget record already exists for period")
    void createBudget_BudgetAlreadyExists_ThrowsResourceConflictException() {
        CreateBudgetRequest request = new CreateBudgetRequest(1L, 2L, BigDecimal.valueOf(300), 5, 2026);

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Dining");

        when(userRepository.findById(request.userId())).thenReturn(Optional.of(user));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(1L, 2L, 5, 2026))
                .thenReturn(Optional.of(new Budget()));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("A budget limit is already defined for category Dining in 5/2026.");

        verify(budgetRepository, never()).save(any());
    }

    // ==========================================
    // updateBudget BRANCHES
    // ==========================================

    @Test
    @DisplayName("updateBudget: Should update limit amount successfully when budget exists")
    void updateBudget_BudgetExists_UpdatesSuccessfully() {
        Long budgetId = 100L;
        UpdateBudgetRequest request = new UpdateBudgetRequest(BigDecimal.valueOf(750));
        Budget existingBudget = createMockBudget(budgetId, 1L, 2L, "Leisure", BigDecimal.valueOf(500), 5, 2026);

        when(budgetRepository.findById(budgetId)).thenReturn(Optional.of(existingBudget));
        when(budgetRepository.save(existingBudget)).thenReturn(existingBudget);

        BudgetResponse response = budgetService.updateBudget(budgetId, request);

        assertThat(response).isNotNull();
        verify(budgetRepository).save(existingBudget);
        assertThat(existingBudget.getLimitAmount()).isEqualTo(BigDecimal.valueOf(750));
    }

    @Test
    @DisplayName("updateBudget: Should throw IllegalArgumentException when budget to update is not found")
    void updateBudget_BudgetNotFound_ThrowsException() {
        Long budgetId = 100L;
        UpdateBudgetRequest request = new UpdateBudgetRequest(BigDecimal.valueOf(750));
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.updateBudget(budgetId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Budget not found with ID:" + budgetId);

        verify(budgetRepository, never()).save(any());
    }

    // ==========================================
    // deleteBudget BRANCHES
    // ==========================================

    @Test
    @DisplayName("deleteBudget: Should delete target budget when it exists")
    void deleteBudget_BudgetExists_DeletesSuccessfully() {
        Long budgetId = 100L;
        when(budgetRepository.existsById(budgetId)).thenReturn(true);

        budgetService.deleteBudget(budgetId);

        verify(budgetRepository).deleteById(budgetId);
    }

    @Test
    @DisplayName("deleteBudget: Should throw IllegalArgumentException when budget record doesn't exist")
    void deleteBudget_BudgetDoesNotExist_ThrowsException() {
        Long budgetId = 100L;
        when(budgetRepository.existsById(budgetId)).thenReturn(false);

        assertThatThrownBy(() -> budgetService.deleteBudget(budgetId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Budget not found with ID: " + budgetId);

        verify(budgetRepository, never()).deleteById(any());
    }

    // ==========================================
    // PRIVATE HELPER METHODS
    // ==========================================

    private Budget createMockBudget(Long id, Long userId, Long categoryId, String categoryName,
                                    BigDecimal limit, Integer month, Integer year) {
        User user = new User();
        user.setId(userId);

        Category category = new Category();
        category.setId(categoryId);
        category.setName(categoryName);

        Budget budget = new Budget();
        budget.setId(id);
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(limit);
        budget.setMonth(month);
        budget.setYear(year);
        return budget;
    }
}