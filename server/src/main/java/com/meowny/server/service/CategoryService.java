package com.meowny.server.service;

import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.User;
import com.meowny.server.dto.category.CategoryResponse;
import com.meowny.server.dto.category.CreateCategoryRequest;
import com.meowny.server.dto.category.UpdateCategoryRequest;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

public class CategoryService {

    private final CategoryRepository categoryRepository;
    private final UserRepository userRepository;
    private final TransactionRepository transactionRepository;
    private final BudgetRepository budgetRepository;

    public CategoryService(CategoryRepository categoryRepository,
                           UserRepository userRepository,
                           TransactionRepository transactionRepository,
                           BudgetRepository budgetRepository) {
        this.categoryRepository = categoryRepository;
        this.userRepository = userRepository;
        this.transactionRepository = transactionRepository;
        this.budgetRepository = budgetRepository;
    }

    @Transactional(readOnly = true)
    public List<CategoryResponse> getCategoriesByUserId(Long userId) {
        return categoryRepository.findByUserId(userId)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public CategoryResponse createCategory(CreateCategoryRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        categoryRepository.findByUserIdAndNameIgnoreCase(request.userId(), request.name())
                .ifPresent(existing -> {
                    throw new ResourceConflictException("A category with the name '" + request.name() + "' already exists.");
                });

        Category category = new Category();
        category.setUser(user);
        category.setType(request.type());
        category.setName(request.name());

        Category savedCategory = categoryRepository.save(category);
        return mapToResponse(savedCategory);
    }

    @Transactional
    public CategoryResponse updateCategoryName(Long id, UpdateCategoryRequest request) {
        Category category = categoryRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + id));

        if (!category.getName().equalsIgnoreCase(request.name())) {
            categoryRepository.findByUserIdAndNameIgnoreCase(category.getUser().getId(), request.name())
                    .ifPresent(existing -> {
                        throw new ResourceConflictException("Another category with the name '" + request.name() + "' already exists.");
                    });
            category.setName(request.name());
        }

        Category updatedCategory = categoryRepository.save(category);
        return mapToResponse(updatedCategory);
    }

    @Transactional
    public void deleteCategory(Long id) {
        Category category = categoryRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + id));

        boolean hasTransactions = transactionRepository.existsByCategoryId(id);
        if (hasTransactions) {
            throw new ResourceConflictException("Cannot delete category because it is linked to existing transactions. Reassign or delete those transactions first.");
        }

        boolean hasBudgets = budgetRepository.existsByCategoryId(id);
        if (hasBudgets) {
            throw new ResourceConflictException("Cannot delete category because it is assigned to an active budget.");
        }

        boolean hasRecurringTransactions = recurringTransactionRepository.existsByCategoryId(id);
        if (hasRecurringTransactions) {
            throw new ResourceConflictException("Cannot delete category because it is linked to active recurring transactions.");
        }

        categoryRepository.delete(category);
    }

    private CategoryResponse mapToResponse(Category category) {
        return new CategoryResponse(
                category.getId(),
                category.getUser().getId(),
                category.getType(),
                category.getName(),
                category.getCreatedAt(),
                category.getUpdatedAt()
        );
    }
}
