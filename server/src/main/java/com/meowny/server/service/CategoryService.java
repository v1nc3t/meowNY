package com.meowny.server.service;

import com.meowny.server.dto.category.CategoryResponse;
import com.meowny.server.dto.category.CreateCategoryRequest;
import com.meowny.server.dto.category.UpdateCategoryRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.CategoryGroup;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.*;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.time.LocalDateTime;
import java.util.List;
import java.util.stream.Collectors;

@Service
public class CategoryService {

    private final CategoryRepository categoryRepository;
    private final CategoryGroupRepository categoryGroupRepository;
    private final UserRepository userRepository;
    private final RecurringTransactionRepository recurringTransactionRepository;
    private final BudgetRepository budgetRepository;

    public CategoryService(CategoryRepository categoryRepository,
                           CategoryGroupRepository categoryGroupRepository,
                           UserRepository userRepository,
                           RecurringTransactionRepository recurringTransactionRepository,
                           BudgetRepository budgetRepository) {
        this.categoryRepository = categoryRepository;
        this.categoryGroupRepository = categoryGroupRepository;
        this.userRepository = userRepository;
        this.recurringTransactionRepository = recurringTransactionRepository;
        this.budgetRepository = budgetRepository;
    }

    @Transactional(readOnly = true)
    public List<CategoryResponse> getCategoriesByUserId(Long userId) {
        return categoryRepository.findByUserIdAndDeletedAtIsNull(userId)
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
        category.setCategoryGroup(resolveCategoryGroup(request.categoryGroupId(), request.userId()));

        Category savedCategory = categoryRepository.save(category);
        return mapToResponse(savedCategory);
    }

    @Transactional
    public CategoryResponse updateCategoryName(Long id, UpdateCategoryRequest request) {
        Category category = findActiveCategory(id);

        if (!category.getName().equalsIgnoreCase(request.name())) {
            categoryRepository.findByUserIdAndNameIgnoreCase(category.getUser().getId(), request.name())
                    .ifPresent(existing -> {
                        throw new ResourceConflictException("Another category with the name '" + request.name() + "' already exists.");
                    });
            category.setName(request.name());
        }

        category.setCategoryGroup(resolveCategoryGroup(request.categoryGroupId(), category.getUser().getId()));

        Category updatedCategory = categoryRepository.save(category);
        return mapToResponse(updatedCategory);
    }

    @Transactional
    public void deleteCategory(Long id) {
        Category category = findActiveCategory(id);

        boolean hasRecurringTransactions = recurringTransactionRepository.existsByCategoryId(id);
        if (hasRecurringTransactions) {
            throw new ResourceConflictException("Cannot delete category because it is linked to active recurring transactions.");
        }

        budgetRepository.deleteByUserIdAndCategoryId(category.getUser().getId(), id);

        category.setDeletedAt(LocalDateTime.now());
        categoryRepository.save(category);
    }

    private Category findActiveCategory(Long id) {
        Category category = categoryRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + id));
        if (category.isDeleted()) {
            throw new IllegalArgumentException("Category not found with ID: " + id);
        }
        return category;
    }

    private CategoryGroup resolveCategoryGroup(Long categoryGroupId, Long userId) {
        if (categoryGroupId == null) {
            return null;
        }

        CategoryGroup group = categoryGroupRepository.findById(categoryGroupId)
                .orElseThrow(() -> new IllegalArgumentException("Category group not found with ID: " + categoryGroupId));

        if (!group.getUser().getId().equals(userId)) {
            throw new IllegalArgumentException("Category group must belong to the specified user.");
        }

        return group;
    }

    private CategoryResponse mapToResponse(Category category) {
        Long groupId = null;
        String groupName = null;
        if (category.getCategoryGroup() != null) {
            groupId = category.getCategoryGroup().getId();
            groupName = category.getCategoryGroup().getName();
        }

        return new CategoryResponse(
                category.getId(),
                category.getUser().getId(),
                groupId,
                groupName,
                category.getType(),
                category.getName(),
                category.getCreatedAt(),
                category.getUpdatedAt()
        );
    }
}
