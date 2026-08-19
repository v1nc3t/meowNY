package com.meowny.server.service;

import com.meowny.server.dto.categorygroup.CategoryGroupResponse;
import com.meowny.server.dto.categorygroup.CreateCategoryGroupRequest;
import com.meowny.server.dto.categorygroup.UpdateCategoryGroupRequest;
import com.meowny.server.entity.CategoryGroup;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.CategoryGroupRepository;
import com.meowny.server.security.CurrentUserService;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

@Service
public class CategoryGroupService {

    private final CategoryGroupRepository categoryGroupRepository;
    private final CurrentUserService currentUserService;

    public CategoryGroupService(
            CategoryGroupRepository categoryGroupRepository,
            CurrentUserService currentUserService) {
        this.categoryGroupRepository = categoryGroupRepository;
        this.currentUserService = currentUserService;
    }

    @Transactional(readOnly = true)
    public List<CategoryGroupResponse> getCurrentUserCategoryGroups() {
        User currentUser = currentUserService.getCurrentUser();
        return categoryGroupRepository.findByUserId(currentUser.getId())
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public CategoryGroupResponse createCategoryGroup(CreateCategoryGroupRequest request) {
        User user = currentUserService.getCurrentUser();
        Long userId = user.getId();

        categoryGroupRepository.findByUserIdAndNameIgnoreCase(userId, request.name())
                .ifPresent(existing -> {
                    throw new ResourceConflictException(
                            "A category group with the name '" + request.name() + "' already exists.");
                });

        CategoryGroup group = new CategoryGroup();
        group.setUser(user);
        group.setName(request.name());

        CategoryGroup savedGroup = categoryGroupRepository.save(group);
        return mapToResponse(savedGroup);
    }

    @Transactional
    public CategoryGroupResponse updateCategoryGroup(Long id, UpdateCategoryGroupRequest request) {
        CategoryGroup group = findOwnedCategoryGroup(id);

        if (!group.getName().equalsIgnoreCase(request.name())) {
            categoryGroupRepository.findByUserIdAndNameIgnoreCase(group.getUser().getId(), request.name())
                    .ifPresent(existing -> {
                        throw new ResourceConflictException(
                                "Another category group with the name '" + request.name() + "' already exists.");
                    });
            group.setName(request.name());
        }

        CategoryGroup updatedGroup = categoryGroupRepository.save(group);
        return mapToResponse(updatedGroup);
    }

    @Transactional
    public void deleteCategoryGroup(Long id) {
        CategoryGroup group = findOwnedCategoryGroup(id);
        categoryGroupRepository.delete(group);
    }

    private CategoryGroup findOwnedCategoryGroup(Long id) {
        CategoryGroup group = categoryGroupRepository.findById(id)
                .orElseThrow(ResourceNotFoundException::new);
        currentUserService.requireOwnedByCurrentUser(group.getUser().getId());
        return group;
    }

    private CategoryGroupResponse mapToResponse(CategoryGroup group) {
        return new CategoryGroupResponse(
                group.getId(),
                group.getUser().getId(),
                group.getName(),
                group.getCreatedAt()
        );
    }
}
