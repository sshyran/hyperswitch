use api_models::user_role as user_role_api;
use diesel_models::user_role::UserRoleUpdate;
use error_stack::ResultExt;
use masking::ExposeInterface;

use crate::{
    core::errors::{UserErrors, UserResponse},
    routes::AppState,
    services::{
        authentication::{self as auth},
        authorization::{info, predefined_permissions},
        ApplicationResponse,
    },
    types::domain,
    utils,
};

pub async fn get_authorization_info(
    _state: AppState,
) -> UserResponse<user_role_api::AuthorizationInfoResponse> {
    Ok(ApplicationResponse::Json(
        user_role_api::AuthorizationInfoResponse(
            info::get_authorization_info()
                .into_iter()
                .filter_map(|module| module.try_into().ok())
                .collect(),
        ),
    ))
}

pub async fn list_roles(_state: AppState) -> UserResponse<user_role_api::ListRolesResponse> {
    Ok(ApplicationResponse::Json(user_role_api::ListRolesResponse(
        predefined_permissions::PREDEFINED_PERMISSIONS
            .iter()
            .filter_map(|(role_id, role_info)| {
                utils::user_role::get_role_name_and_permission_response(role_info).map(
                    |(permissions, role_name)| user_role_api::RoleInfoResponse {
                        permissions,
                        role_id,
                        role_name,
                    },
                )
            })
            .collect(),
    )))
}

pub async fn get_role(
    _state: AppState,
    role: user_role_api::GetRoleRequest,
) -> UserResponse<user_role_api::RoleInfoResponse> {
    let info = predefined_permissions::PREDEFINED_PERMISSIONS
        .get_key_value(role.role_id.as_str())
        .and_then(|(role_id, role_info)| {
            utils::user_role::get_role_name_and_permission_response(role_info).map(
                |(permissions, role_name)| user_role_api::RoleInfoResponse {
                    permissions,
                    role_id,
                    role_name,
                },
            )
        })
        .ok_or(UserErrors::InvalidRoleId)?;

    Ok(ApplicationResponse::Json(info))
}

pub async fn update_user_role(
    state: AppState,
    user_from_token: auth::UserFromToken,
    req: user_role_api::UpdateUserRoleRequest,
) -> UserResponse<()> {
    let merchant_id = user_from_token.merchant_id;
    let role_id = req.role_id.clone();
    utils::user_role::validate_role_id(role_id.as_str())?;

    if user_from_token.user_id == req.user_id {
        return Err(UserErrors::InvalidRoleOperation.into())
            .attach_printable("Admin User Changing their role");
    }

    state
        .store
        .update_user_role_by_user_id_merchant_id(
            req.user_id.as_str(),
            merchant_id.as_str(),
            UserRoleUpdate::UpdateRole {
                role_id,
                modified_by: user_from_token.user_id,
            },
        )
        .await
        .map_err(|e| {
            if e.current_context().is_db_not_found() {
                return e
                    .change_context(UserErrors::InvalidRoleOperation)
                    .attach_printable("UserId MerchantId not found");
            }
            e.change_context(UserErrors::InternalServerError)
        })?;

    Ok(ApplicationResponse::StatusOk)
}

pub async fn delete_user_role(
    state: AppState,
    user_from_token: auth::UserFromToken,
    request: user_role_api::DeleteUserRoleRequest,
) -> UserResponse<()> {
    let user_from_db: domain::UserFromStorage = state
        .store
        .find_user_by_email(
            domain::UserEmail::from_pii_email(request.email)?
                .get_secret()
                .expose()
                .as_str(),
        )
        .await
        .map_err(|e| {
            if e.current_context().is_db_not_found() {
                e.change_context(UserErrors::InvalidRoleOperation)
                    .attach_printable("User not found in records")
            } else {
                e.change_context(UserErrors::InternalServerError)
            }
        })?
        .into();

    if user_from_db.get_user_id() == user_from_token.user_id {
        return Err(UserErrors::InvalidDeleteOperation.into())
            .attach_printable("User deleting himself");
    }

    let user_roles = state
        .store
        .list_user_roles_by_user_id(user_from_db.get_user_id())
        .await
        .change_context(UserErrors::InternalServerError)?;

    match user_roles
        .iter()
        .find(|&role| role.merchant_id == user_from_token.merchant_id.as_str())
    {
        Some(user_role) => {
            utils::user::validate_deletion_permission_for_role_id(&user_role.role_id)?;
        }
        None => {
            return Err(UserErrors::InvalidDeleteOperation.into())
                .attach_printable("User is not associated with the merchant");
        }
    };

    if user_roles.len() > 1 {
        state
            .store
            .delete_user_role_by_user_id_merchant_id(
                user_from_db.get_user_id(),
                user_from_token.merchant_id.as_str(),
            )
            .await
            .change_context(UserErrors::InternalServerError)
            .attach_printable("Error while deleting user role")?;

        Ok(ApplicationResponse::StatusOk)
    } else {
        state
            .store
            .delete_user_by_user_id(user_from_db.get_user_id())
            .await
            .change_context(UserErrors::InternalServerError)
            .attach_printable("Error while deleting user entry")?;

        state
            .store
            .delete_user_role_by_user_id_merchant_id(
                user_from_db.get_user_id(),
                user_from_token.merchant_id.as_str(),
            )
            .await
            .change_context(UserErrors::InternalServerError)
            .attach_printable("Error while deleting user role")?;

        Ok(ApplicationResponse::StatusOk)
    }
}
