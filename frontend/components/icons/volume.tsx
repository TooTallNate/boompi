import { SpeakerLoudIcon, SpeakerModerateIcon, SpeakerOffIcon, SpeakerQuietIcon } from './radix';

interface VolumeParams extends React.ComponentPropsWithoutRef<'svg'> {
	mute?: boolean;
	level?: number;
}

export default function Volume({ mute = false, level = 0, ...props }: VolumeParams) {
	if (mute) {
		return <SpeakerOffIcon {...props} />
	} else if (level === 3) {
		return <SpeakerLoudIcon {...props} />
	} else if (level === 2) {
		return <SpeakerModerateIcon {...props} />
	} else if (level === 1) {
		return <SpeakerQuietIcon {...props} />
	}
	return <SpeakerOffIcon {...props} />
}
