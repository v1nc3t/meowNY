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
import com.meowny.server.support.TestCurrentUserSupport;
import org.junit.jupiter.api.BeforeEach;
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
import static org.mockito.ArgumentMatchers.anyLong;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class CategoryGroupServiceTest {

    @Mock
    private CategoryGroupRepository categoryGroupRepository;

    @Mock
    private CurrentUserService currentUserService;

    @InjectMocks
    private CategoryGroupService categoryGroupService;

    private User currentUser;

    @BeforeEach
    void setUp() {
        currentUser = new User();
        currentUser.setId(1L);
        TestCurrentUserSupport.stubCurrentUser(currentUserService, currentUser);
    }

    @Test
    @DisplayName("getCurrentUserCategoryGroups: Should return list of group responses")
    void getCurrentUserCategoryGroups_ValidUser_ReturnsList() {
        CategoryGroup group = createMockGroup(10L, 1L, "Essentials");
        when(categoryGroupRepository.findByUserId(1L)).thenReturn(List.of(group));

        List<CategoryGroupResponse> results = categoryGroupService.getCurrentUserCategoryGroups();

        assertThat(results).hasSize(1);
        assertThat(results.get(0).name()).isEqualTo("Essentials");
    }

    @Test
    @DisplayName("createCategoryGroup: Should create group successfully when valid and unique")
    void createCategoryGroup_ValidRequest_CreatesGroup() {
        CreateCategoryGroupRequest request = new CreateCategoryGroupRequest("Lifestyle");
        CategoryGroup savedGroup = createMockGroup(10L, 1L, "Lifestyle");

        when(categoryGroupRepository.findByUserIdAndNameIgnoreCase(1L, "Lifestyle")).thenReturn(Optional.empty());
        when(categoryGroupRepository.save(any(CategoryGroup.class))).thenReturn(savedGroup);

        CategoryGroupResponse response = categoryGroupService.createCategoryGroup(request);

        assertThat(response.id()).isEqualTo(10L);
        verify(categoryGroupRepository).save(any(CategoryGroup.class));
    }

    @Test
    @DisplayName("createCategoryGroup: Should throw ResourceConflictException when name already exists")
    void createCategoryGroup_NameExists_ThrowsResourceConflictException() {
        CreateCategoryGroupRequest request = new CreateCategoryGroupRequest("Lifestyle");
        when(categoryGroupRepository.findByUserIdAndNameIgnoreCase(1L, "Lifestyle"))
                .thenReturn(Optional.of(new CategoryGroup()));

        assertThatThrownBy(() -> categoryGroupService.createCategoryGroup(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("A category group with the name 'Lifestyle' already exists.");
    }

    @Test
    @DisplayName("updateCategoryGroup: Should throw ResourceNotFoundException when group does not exist")
    void updateCategoryGroup_NotFound_ThrowsException() {
        UpdateCategoryGroupRequest request = new UpdateCategoryGroupRequest("New Name");
        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryGroupService.updateCategoryGroup(10L, request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("updateCategoryGroup: Should throw ResourceNotFoundException when group belongs to another user")
    void updateCategoryGroup_OtherUsersGroup_ThrowsException() {
        UpdateCategoryGroupRequest request = new UpdateCategoryGroupRequest("Housing");
        CategoryGroup existing = createMockGroup(10L, 2L, "Essentials");
        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.of(existing));

        assertThatThrownBy(() -> categoryGroupService.updateCategoryGroup(10L, request))
                .isInstanceOf(ResourceNotFoundException.class);

        verify(categoryGroupRepository, never()).save(any());
    }

    @Test
    @DisplayName("updateCategoryGroup: Should save without uniqueness check if name is unchanged")
    void updateCategoryGroup_NameUnchanged_SavesImmediately() {
        UpdateCategoryGroupRequest request = new UpdateCategoryGroupRequest("essentials");
        CategoryGroup existing = createMockGroup(10L, 1L, "Essentials");

        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.of(existing));
        when(categoryGroupRepository.save(existing)).thenReturn(existing);

        CategoryGroupResponse response = categoryGroupService.updateCategoryGroup(10L, request);

        assertThat(response).isNotNull();
        verify(categoryGroupRepository, never()).findByUserIdAndNameIgnoreCase(anyLong(), anyString());
    }

    @Test
    @DisplayName("updateCategoryGroup: Should update name when it is free")
    void updateCategoryGroup_NameChangedAndFree_UpdatesSuccessfully() {
        UpdateCategoryGroupRequest request = new UpdateCategoryGroupRequest("Housing");
        CategoryGroup existing = createMockGroup(10L, 1L, "Essentials");

        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.of(existing));
        when(categoryGroupRepository.findByUserIdAndNameIgnoreCase(1L, "Housing")).thenReturn(Optional.empty());
        when(categoryGroupRepository.save(existing)).thenReturn(existing);

        categoryGroupService.updateCategoryGroup(10L, request);

        assertThat(existing.getName()).isEqualTo("Housing");
    }

    @Test
    @DisplayName("updateCategoryGroup: Should throw ResourceConflictException when new name is taken")
    void updateCategoryGroup_NameChangedAndTaken_ThrowsResourceConflictException() {
        UpdateCategoryGroupRequest request = new UpdateCategoryGroupRequest("Housing");
        CategoryGroup existing = createMockGroup(10L, 1L, "Essentials");

        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.of(existing));
        when(categoryGroupRepository.findByUserIdAndNameIgnoreCase(1L, "Housing"))
                .thenReturn(Optional.of(new CategoryGroup()));

        assertThatThrownBy(() -> categoryGroupService.updateCategoryGroup(10L, request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Another category group with the name 'Housing' already exists.");
    }

    @Test
    @DisplayName("deleteCategoryGroup: Should delete when the group exists and is owned")
    void deleteCategoryGroup_Exists_DeletesSuccessfully() {
        CategoryGroup existing = createMockGroup(10L, 1L, "Essentials");
        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.of(existing));

        categoryGroupService.deleteCategoryGroup(10L);

        verify(categoryGroupRepository).delete(existing);
    }

    @Test
    @DisplayName("deleteCategoryGroup: Should throw ResourceNotFoundException when group does not exist")
    void deleteCategoryGroup_NotFound_ThrowsException() {
        when(categoryGroupRepository.findById(10L)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> categoryGroupService.deleteCategoryGroup(10L))
                .isInstanceOf(ResourceNotFoundException.class);

        verify(categoryGroupRepository, never()).delete(any());
    }

    private CategoryGroup createMockGroup(Long id, Long userId, String name) {
        User user = new User();
        user.setId(userId);

        CategoryGroup group = new CategoryGroup();
        group.setId(id);
        group.setUser(user);
        group.setName(name);
        return group;
    }
}
