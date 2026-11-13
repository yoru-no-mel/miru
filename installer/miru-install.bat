@echo off
setlocal enableextensions enabledelayedexpansion
path %SystemRoot%\System32;%SystemRoot%;%SystemRoot%\System32\Wbem;%SystemRoot%\System32\WindowsPowerShell\v1.0\

:: Unattended install flag. When set, the script will not require user input.
set unattended=no
if "%1"=="/u" set unattended=yes

:: Make sure this is Windows Vista or later
call :ensure_vista

:: Make sure the script is running as admin
call :ensure_admin

:: Command line arguments to use when launching miru from a file association
set miru_args=

:: Get miru.exe location
cd /D %~dp0\..
set miru_path=%cd%\miru.exe
if not exist "%miru_path%" call :die "miru.exe not found"

:: Register miru.exe under the "App Paths" key, so it can be found by
:: ShellExecute, the run command, the start menu, etc.
set app_paths_key=HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\miru.exe
call :reg add "%app_paths_key%" /d "%miru_path%" /f
call :reg add "%app_paths_key%" /v "UseUrl" /t REG_DWORD /d 1 /f

:: Register miru.exe under the "Applications" key to add some default verbs for
:: when miru is used from the "Open with" menu
set classes_root_key=HKLM\SOFTWARE\Classes
set app_key=%classes_root_key%\Applications\miru.exe
call :reg add "%app_key%" /v "FriendlyAppName" /d "miru" /f
call :add_verbs "%app_key%"

:: Add miru to the "Open with" list for all image file types
call :reg add "%classes_root_key%\SystemFileAssociations\image\OpenWithList\miru.exe" /d "" /f

:: Add a capabilities key for miru, which is registered later on for use in the
:: "Default Programs" control panel
set capabilities_key=HKLM\SOFTWARE\Clients\Media\miru\Capabilities
call :reg add "%capabilities_key%" /v "ApplicationName" /d "miru" /f
call :reg add "%capabilities_key%" /v "ApplicationDescription" /d "miru image viewer" /f

:: Add file types
set supported_types_key=%app_key%\SupportedTypes
set file_associations_key=%capabilities_key%\FileAssociations
:: Raster image formats
call :add_type "image/png"                        "image" "PNG Image"                  ".png"
call :add_type "image/jpeg"                       "image" "JPEG Image"                 ".jpg" ".jpeg"
call :add_type "image/gif"                        "image" "GIF Image"                  ".gif"
call :add_type "image/bmp"                        "image" "Bitmap Image"              ".bmp"
call :add_type "image/tiff"                       "image" "TIFF Image"                 ".tif" ".tiff"
call :add_type "image/webp"                       "image" "WebP Image"                 ".webp"
:: Archive formats
call :add_type "application/vnd.comicbook+zip"    ""      "Comic Book Archive"         ".cbz"
call :add_type "application/vnd.comicbook-rar"    ""      "Comic Book Archive (RAR)"   ".cbr"

:: Register "Default Programs" entry
call :reg add "HKLM\SOFTWARE\RegisteredApplications" /v "miru" /d "SOFTWARE\Clients\Media\miru\Capabilities" /f

:: Add start menu link
powershell "$s=(New-Object -COM WScript.Shell).CreateShortcut('%ProgramData%\Microsoft\Windows\Start Menu\Programs\miru.lnk');$s.TargetPath='%miru_path%';$s.Save()"

echo.
echo Installed successfully^^! You can now configure miru's file associations in
echo the Default Programs control panel.
echo.
if [%unattended%] == [yes] exit 0
<nul set /p =Press any key to open the Default Programs control panel . . .
pause >nul
control /name Microsoft.DefaultPrograms
exit 0

:die
	if not [%1] == [] echo %~1
	if [%unattended%] == [yes] exit 1
	pause
	exit 1

:ensure_admin
	:: 'openfiles' is just a commmand that is present on all supported Windows
	:: versions, requires admin privileges and has no side effects, see:
	:: https://stackoverflow.com/questions/4051883/batch-script-how-to-check-for-admin-rights
	openfiles >nul 2>&1
	if errorlevel 1 (
		echo This batch script requires administrator privileges. Right-click on
		echo miru-install.bat and select "Run as administrator".
		call :die
	)
	goto :EOF

:ensure_vista
	ver | find "XP" >nul
	if not errorlevel 1 (
		echo This batch script only works on Windows Vista and later. To create file
		echo associations on Windows XP, right click on an image file and use "Open with...".
		call :die
	)
	goto :EOF

:reg
	:: Wrap the reg command to check for errors
	>nul reg %*
	if errorlevel 1 set error=yes
	if [%error%] == [yes] echo Error in command: reg %*
	if [%error%] == [yes] call :die
	goto :EOF

:reg_set_opt
	:: Set a value in the registry if it doesn't already exist
	set key=%~1
	set value=%~2
	set data=%~3

	reg query "%key%" /v "%value%" >nul 2>&1
	if errorlevel 1 call :reg add "%key%" /v "%value%" /d "%data%"
	goto :EOF

:add_verbs
	set key=%~1

	:: Set the default verb to "open"
	call :reg add "%key%\shell" /d "open" /f

	:: Set open command
	call :reg add "%key%\shell\open" /d "&Open" /f
	call :reg add "%key%\shell\open\command" /d "\"%miru_path%\" %miru_args% \"%%%%L\"" /f

	goto :EOF

:add_progid
	set prog_id=%~1
	set friendly_name=%~2

	:: Add ProgId, edit flags are FTA_OpenIsSafe
	set prog_id_key=%classes_root_key%\%prog_id%
	call :reg add "%prog_id_key%" /d "%friendly_name%" /f
	call :reg add "%prog_id_key%" /v "EditFlags" /t REG_DWORD /d 65536 /f
	call :reg add "%prog_id_key%" /v "FriendlyTypeName" /d "%friendly_name%" /f
	call :reg add "%prog_id_key%\DefaultIcon" /d "\"%miru_path%\",0" /f
	call :add_verbs "%prog_id_key%"

	goto :EOF

:update_extension
	set extension=%~1
	set prog_id=%~2
	set mime_type=%~3
	set perceived_type=%~4

	:: Add information about the file extension, if not already present
	set extension_key=%classes_root_key%\%extension%
	if not [%mime_type%] == [] call :reg_set_opt "%extension_key%" "Content Type" "%mime_type%"
	if not [%perceived_type%] == [] call :reg_set_opt "%extension_key%" "PerceivedType" "%perceived_type%"
	call :reg add "%extension_key%\OpenWithProgIds" /v "%prog_id%" /f

	:: Add type to SupportedTypes
	call :reg add "%supported_types_key%" /v "%extension%" /f

	:: Add type to the Default Programs control panel
	call :reg add "%file_associations_key%" /v "%extension%" /d "%prog_id%" /f

	goto :EOF

:add_type
	set mime_type=%~1
	set perceived_type=%~2
	set friendly_name=%~3
	set extension=%~4

	echo Adding "%extension%" file type

	:: Add ProgId
	set prog_id=io.miru%extension%
	call :add_progid "%prog_id%" "%friendly_name%"

	:: Add extensions
	:extension_loop
		call :update_extension "%extension%" "%prog_id%" "%mime_type%" "%perceived_type%"

		:: Trailing parameters are additional extensions
		shift /4
		set extension=%~4
		if not [%extension%] == [] goto extension_loop

	goto :EOF
